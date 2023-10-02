use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;
use patch::Patch;

use super::{Hunk, Line, Repo};

// Returns the names of non-deleted staged files.
const STAGED_FILES_CMD: &[&str] = &["diff", "--cached", "--name-only", "--diff-filter=d"];
// Returns all staged hunks for non-deleted files.
// --no-prefix omits the path prefix for the old and new files (a/ and b/, respectively).
const STAGED_HUNKS_CMD: &[&str] = &["diff", "--cached", "--no-prefix", "--diff-filter=d"];

pub struct Cli;

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
            let hunks = diff_file.hunks.iter().map(Hunk::from).collect();
            hunk_map.insert(path, hunks);
        }

        Ok(hunk_map)
    }
}

impl From<&patch::Hunk<'_>> for Hunk {
    fn from(h: &patch::Hunk) -> Self {
        let mut lines = Vec::new();
        let mut num_added = 0;
        let mut num_removed = 0;
        let mut num_context = 0;
        let (start_line, end_line) = (
            h.new_range.start as u32,
            (h.new_range.start + h.new_range.count - 1) as u32,
        );
        let old_start_line = h.old_range.start as u32;
        for line in &h.lines {
            let new_line = start_line + num_context + num_added;
            let old_line = old_start_line + num_context + num_removed;
            let line = match line {
                patch::Line::Add(_) => {
                    num_added += 1;
                    Line::Add(new_line)
                }
                patch::Line::Remove(_) => {
                    num_removed += 1;
                    Line::Remove(old_line)
                }
                patch::Line::Context(_) => {
                    num_context += 1;
                    Line::Context(old_line, new_line)
                }
            };
            lines.push(line);
        }
        Self {
            lines,
            start_line,
            end_line,
        }
    }
}
