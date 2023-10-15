use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;
use patch::Patch;

use super::{Hunk, Line, Repo};

// Returns the names of non-deleted staged files.
const STAGED_FILES_CMD: &[&str] = &[
    "diff",
    "--cached",
    "--name-only",
    // Render paths relative to pwd.
    "--relative",
    // Ignore deleted files.
    "--diff-filter=d",
];
// Returns all staged hunks for non-deleted files.
const STAGED_HUNKS_CMD: &[&str] = &[
    "diff",
    "--cached",
    // Render paths relative to pwd.
    "--relative",
    // Omits the path prefix for the old and new files (a/ and b/, respectively).
    "--no-prefix",
    // Ignore deleted files.
    "--diff-filter=d",
];

pub struct Cli<'a> {
    pub repo_path: &'a Path,
}

impl<'a> Repo for Cli<'a> {
    fn get_staged_files(&self) -> Result<Vec<PathBuf>> {
        let output = Command::new("git")
            .current_dir(self.repo_path)
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
            let p = Path::new(p);
            let absolute = self.repo_path.join(p).canonicalize()?;
            if !absolute.exists() {
                return Err(anyhow::anyhow!(
                    "file {} does not exist",
                    absolute.display()
                ));
            }
            paths.push(p.to_owned());
        }

        Ok(paths)
    }

    fn get_staged_hunks(&self) -> Result<BTreeMap<PathBuf, Vec<Hunk>>> {
        let output = Command::new("git")
            .current_dir(self.repo_path)
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

        if stdout.trim().is_empty() {
            return Ok(hunk_map);
        }

        // TODO(aksiksi): Handle deleted files, binary files, etc.
        let patch = Patch::from_multiple(stdout).map_err(|e| anyhow::anyhow!("{}", e))?;
        for diff_file in patch {
            // We only look at the new file.
            let path = PathBuf::from(diff_file.new.path.as_ref());
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
