use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;

pub mod cli;
#[cfg(feature = "git")]
mod lib;

pub trait Repo {
    fn get_staged_files(&self, repo_path: Option<&Path>) -> Result<(Vec<PathBuf>, PathBuf)>;
    fn get_staged_hunks(&self, repo_path: Option<&Path>) -> Result<BTreeMap<PathBuf, Vec<Hunk>>>;
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
