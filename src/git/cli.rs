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

            let mut hunks = Vec::new();
            for h in diff_file.hunks {
                let mut hunk = Hunk::from(&h);
                for line in h.lines {
                    hunk.handle_line(line.into());
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
            old_start_line: h.old_range.start as u32,
            old_end_line: (h.old_range.start + h.old_range.count - 1) as u32,
            changed_lines: Vec::new(),
            num_added: 0,
            num_removed: 0,
            num_context: 0,
        }
    }
}

impl From<patch::Line<'_>> for Line {
    fn from(value: patch::Line<'_>) -> Self {
        match value {
            patch::Line::Add(_) => Line::Add,
            patch::Line::Remove(_) => Line::Remove,
            patch::Line::Context(_) => Line::Context,
        }
    }
}
