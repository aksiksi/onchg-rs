use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use anyhow::Result;
use git2::{Delta, DiffHunk, DiffLine, Repository, StatusOptions};

use super::{Hunk, Line, Repo};

impl From<DiffHunk<'_>> for Hunk {
    fn from(h: DiffHunk<'_>) -> Self {
        Self {
            lines: Vec::new(),
            start_line: h.new_start(),
            end_line: h.new_start() + h.new_lines() - 1,
        }
    }
}

impl From<DiffLine<'_>> for Line {
    fn from(l: DiffLine<'_>) -> Self {
        match l.origin() {
            '+' => Line::Add(l.new_lineno().unwrap()),
            '-' => Line::Remove(l.old_lineno().unwrap()),
            ' ' => Line::Context(l.old_lineno().unwrap(), l.new_lineno().unwrap()),
            _ => unreachable!(),
        }
    }
}

impl Repo for Repository {
    fn get_staged_files(&self) -> Result<Vec<PathBuf>> {
        let mut opts = StatusOptions::new();
        let mut paths = Vec::new();
        for entry in self
            .statuses(Some(opts.show(git2::StatusShow::Index)))?
            .iter()
        {
            match entry.status() {
                // We only care about modified and new files.
                git2::Status::INDEX_NEW | git2::Status::INDEX_MODIFIED => (),
                _ => continue,
            }
            let file_path = match entry.path() {
                Some(p) => p,
                None => continue,
            };
            paths.push(PathBuf::from(file_path));
        }
        Ok(paths)
    }

    // NOTE(aksiksi): This is 2x slower than the CLI-based diff.
    //
    // For the "repo" benchmark, which consists of ~2300 diff lines, the CLI takes ~250ms,
    // while libgit2 takes ~500ms.
    //
    // Based on my analysis, it takes no more than 15us to process each line. The combination
    // of getting the tree and computing the diff is 300us. This implies that most of the time
    // is spent *in between* each line callback.
    //
    // But is there even another way to get hunk content? Based on the API, using the line_cb is
    // the only way to see diff content.
    fn get_staged_hunks(&self) -> Result<BTreeMap<PathBuf, Vec<Hunk>>> {
        let mut hunk_map: BTreeMap<PathBuf, HashMap<(u32, u32), Hunk>> = BTreeMap::new();

        let s = std::time::Instant::now();
        let tree = self.head()?.peel_to_tree()?;
        log::info!("Got tree in {:?}", s.elapsed());

        let s = std::time::Instant::now();
        let diff = self.diff_tree_to_index(Some(&tree), None, None)?;
        log::info!("Diff tree to index in {:?}", s.elapsed());

        let s = std::time::Instant::now();
        let mut num_lines = 0;

        diff.foreach(
            &mut |_delta, _progress| true,
            None,
            None,
            Some(&mut |delta, raw_hunk, line| {
                let s = std::time::Instant::now();
                if raw_hunk.is_none() {
                    return true;
                }
                let raw_hunk = raw_hunk.unwrap();
                let valid = if let Delta::Added | Delta::Modified = delta.status() {
                    true
                } else {
                    false
                };
                if !valid {
                    return true;
                }
                match line.origin() {
                    '+' | '-' | ' ' => (),
                    _ => return true,
                }

                let file_path = delta
                    .new_file()
                    .path()
                    .expect("no new file provided")
                    .to_owned();

                let this_hunk = Hunk::from(raw_hunk);
                let (start_line, end_line) = (this_hunk.start_line, this_hunk.end_line);

                if !hunk_map.contains_key(&file_path) {
                    hunk_map.insert(file_path.clone(), HashMap::new());
                }
                let file_map = hunk_map.get_mut(&file_path).unwrap();
                if !file_map.contains_key(&(start_line, end_line)) {
                    file_map.insert((start_line, end_line), this_hunk);
                }

                file_map
                    .get_mut(&(start_line, end_line))
                    .unwrap()
                    .lines
                    .push(line.into());

                // Only log timing for the 1st line.
                if num_lines == 0 {
                    log::info!("Handled line in {:?}", s.elapsed());
                }
                num_lines += 1;

                true
            }),
        )?;

        log::info!("Processed {} lines in {:?}", num_lines, s.elapsed());

        let hunk_map = hunk_map
            .into_iter()
            .map(|(k, v)| (k, v.into_values().collect()))
            .collect();

        Ok(hunk_map)
    }
}
