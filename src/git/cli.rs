use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;
use patch::Patch;

use super::{Hunk, Repo};

pub struct Cli;

impl Cli {
    #[allow(unused)]
    pub fn new() -> Self {
        Self {}
    }
}

// Returns the names of non-deleted staged files.
const STAGED_FILES_CMD: &[&str] = &["diff", "--cached", "--name-only", "--diff-filter=d"];
// Returns all staged hunks for non-deleted files.
// --no-prefix omits the path prefix for the old and new files (a/ and b/, respectively).
const STAGED_HUNKS_CMD: &[&str] = &["diff", "--cached", "--no-prefix", "--diff-filter=d"];

impl Repo for Cli {
    fn get_staged_files(&self, repo_path: Option<&Path>) -> Result<(Vec<PathBuf>, PathBuf)> {
        let repo_path = match repo_path {
            Some(p) => p.to_owned(),
            None => std::env::current_dir()?,
        };
        let output = Command::new("git")
            .current_dir(&repo_path)
            .args(STAGED_FILES_CMD)
            .output()?;
        let (stdout, stderr) = (
            std::str::from_utf8(&output.stdout)?,
            std::str::from_utf8(&output.stderr)?,
        );
        if !output.status.success() {
            return Err(anyhow::anyhow!("git diff failed: {}", stderr));
        }

        let mut paths = Vec::new();
        for p in stdout.split("\n") {
            let p = p.trim();
            if p.is_empty() {
                continue;
            }
            let path = repo_path.join(Path::new(p)).canonicalize()?;
            if !path.exists() {
                return Err(anyhow::anyhow!("file {} does not exist", path.display()));
            }
            paths.push(path);
        }

        Ok((paths, repo_path))
    }

    fn get_staged_hunks(&self, repo_path: Option<&Path>) -> Result<BTreeMap<PathBuf, Vec<Hunk>>> {
        let repo_path = match repo_path {
            Some(p) => p.to_owned(),
            None => std::env::current_dir()?,
        };
        let output = Command::new("git")
            .current_dir(&repo_path)
            .args(STAGED_HUNKS_CMD)
            .output()?;
        let (raw_stdout, raw_stderr) = (output.stdout, output.stderr);
        let (stdout, stderr) = (
            std::str::from_utf8(&raw_stdout)?,
            std::str::from_utf8(&raw_stderr)?,
        );
        if !output.status.success() {
            return Err(anyhow::anyhow!("git diff failed: {}", stderr));
        }
        let mut hunk_map: BTreeMap<PathBuf, Vec<Hunk>> = BTreeMap::new();

        // TODO(aksiksi): Handle deleted files, binary files, etc.
        let patch = Patch::from_multiple(stdout).map_err(|e| anyhow::anyhow!("{}", e))?;
        for diff_file in patch {
            // We only look at the new file.
            let path = repo_path
                .join(Path::new(diff_file.new.path.as_ref()))
                .canonicalize()?;

            let mut hunks = Vec::new();
            for h in diff_file.hunks {
                let mut hunk = Hunk::from(&h);
                let (mut old_line, mut new_line) =
                    (h.old_range.start as u32, h.new_range.start as u32);
                for line in h.lines {
                    // Same logic as in the lib module.
                    match line {
                        patch::Line::Add(_) => {
                            hunk.changed_lines.push(new_line);
                            new_line += 1;
                            hunk.num_added += 1;
                        }
                        patch::Line::Remove(_) => {
                            hunk.num_removed += 1;
                            old_line += 1;
                        }
                        patch::Line::Context(_) => {
                            if u32::abs_diff(old_line, new_line)
                                != u32::abs_diff(hunk.num_added, hunk.num_removed)
                            {
                                hunk.changed_lines.push(new_line);
                            }
                            old_line += 1;
                            new_line += 1;
                        }
                    }
                }
                hunks.push(hunk);
            }

            hunk_map.insert(path, hunks);
        }

        Ok(hunk_map)
    }
}

impl From<&patch::Hunk<'_>> for Hunk {
    fn from(h: &patch::Hunk) -> Self {
        Self {
            start_line: h.new_range.start as u32,
            end_line: (h.new_range.start + h.new_range.count - 1) as u32,
            changed_lines: Vec::new(),
            num_added: 0,
            num_removed: 0,
        }
    }
}
