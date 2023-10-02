use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

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
    fn get_staged_files(&self, repo_path: Option<&Path>) -> Result<(Vec<PathBuf>, PathBuf)> {
        let repo_path = match repo_path {
            Some(p) => p,
            None => self.path().parent().unwrap(),
        };

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
            paths.push(repo_path.join(Path::new(file_path)));
        }

        Ok((paths, repo_path.to_owned()))
    }

    // NOTE(aksiksi): We can probably filter out irrelevant hunks here if we look at
    // the blocks in the FileSet.
    fn get_staged_hunks(&self, repo_path: Option<&Path>) -> Result<BTreeMap<PathBuf, Vec<Hunk>>> {
        let repo_path = match repo_path {
            Some(p) => p,
            None => self.path().parent().unwrap(),
        };

        let mut hunk_map: BTreeMap<PathBuf, HashMap<(u32, u32), Hunk>> = BTreeMap::new();
        let tree = self.head()?.peel_to_tree()?;
        let diff = self.diff_tree_to_index(Some(&tree), None, None)?;
        diff.foreach(
            &mut |_delta, _progress| true,
            None,
            None,
            Some(&mut |delta, raw_hunk, line| {
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
                let file_path = repo_path.join(file_path);

                let this_hunk = Hunk::from(raw_hunk);
                let (start_line, end_line) = (this_hunk.start_line, this_hunk.end_line);

                if !hunk_map.contains_key(&file_path) {
                    let mut m = HashMap::new();
                    m.insert((start_line, end_line), this_hunk);
                    hunk_map.insert(file_path.clone(), m);
                }

                let hunk = hunk_map
                    .get_mut(&file_path)
                    .unwrap()
                    .get_mut(&(start_line, end_line))
                    .unwrap();

                hunk.lines.push(line.into());

                true
            }),
        )?;

        let hunk_map = hunk_map
            .into_iter()
            .map(|(k, v)| (k, v.into_values().collect()))
            .collect();

        Ok(hunk_map)
    }
}
