use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use anyhow::Result;
use git2::{Delta, DiffHunk, Repository, StatusOptions};

pub fn get_staged_file_paths<P: AsRef<Path>>(
    path: P,
    is_repo_path: bool,
) -> Result<(Vec<PathBuf>, PathBuf, Repository)> {
    let repo = if is_repo_path {
        Repository::open(path.as_ref())?
    } else {
        Repository::discover(path.as_ref())?
    };
    let repo_path = repo.path().parent().unwrap();
    let mut opts = StatusOptions::new();
    let mut paths = Vec::new();
    for entry in repo
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

    Ok((paths, repo_path.to_owned(), repo))
}

#[derive(Debug)]
pub struct Hunk {
    pub start_line: u32,
    pub end_line: u32,
    pub changed_lines: Vec<u32>,
    pub num_added: u32,
    pub num_removed: u32,
}

impl Hunk {
    pub fn is_line_changed_within(&self, start: u32, end: u32) -> bool {
        for line in &self.changed_lines {
            if *line > start && *line < end {
                return true;
            }
        }
        false
    }
}

impl From<DiffHunk<'_>> for Hunk {
    fn from(h: DiffHunk<'_>) -> Self {
        Self {
            start_line: h.new_start(),
            end_line: h.new_start() + h.new_lines() - 1,
            changed_lines: Vec::new(),
            num_added: 0,
            num_removed: 0,
        }
    }
}

// NOTE(aksiksi): We can probably filter out irrelevant hunks here if we look at
// the blocks in the FileSet.
pub fn get_staged_hunks(repo: &Repository) -> Result<BTreeMap<PathBuf, Vec<Hunk>>> {
    let repo_path = repo.path().parent().unwrap();
    let mut hunk_map: BTreeMap<PathBuf, HashMap<(u32, u32), Hunk>> = BTreeMap::new();
    let tree = repo.head()?.peel_to_tree()?;
    let diff = repo.diff_tree_to_index(Some(&tree), None, None)?;
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
            // Ignore unchanged lines.
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

            // TODO(aksiksi): Explain this logic.
            let (changed_line, num_added, num_removed) =
                match (line.origin(), line.old_lineno(), line.new_lineno()) {
                    ('+', _, Some(new_line)) => (Some(new_line), 1, 0),
                    ('-', Some(_), _) => (None, 0, 1),
                    (' ', Some(old_line), Some(new_line)) => {
                        if u32::abs_diff(old_line, new_line)
                            == u32::abs_diff(hunk.num_added, hunk.num_removed)
                        {
                            (None, 0, 0)
                        } else {
                            (Some(new_line), 0, 0)
                        }
                    }
                    _ => return true,
                };

            if let Some(changed_line) = changed_line {
                hunk.changed_lines.push(changed_line);
            }
            hunk.num_added += num_added;
            hunk.num_removed += num_removed;

            true
        }),
    )?;

    let hunk_map = hunk_map
        .into_iter()
        .map(|(k, v)| (k, v.into_values().collect()))
        .collect();

    Ok(hunk_map)
}
