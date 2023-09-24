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
    pub old_start_line: u32,
    pub old_end_line: u32,
    pub changed_lines: Vec<u32>,
    pub num_added: u32,
    pub num_removed: u32,
    pub num_context: u32,
}

impl Hunk {
    pub fn handle_line(&mut self, line: Line) {
        match line {
            Line::Add => {
                self.num_added += 1;
                self.changed_lines.push(self.num_context + self.num_added);
            }
            Line::Remove => self.num_removed += 1,
            Line::Context => {
                let is_hunk_end = self.old_start_line + self.num_removed + self.num_context
                    == self.old_end_line
                    && self.start_line + self.num_added + self.num_context == self.end_line;
                if !is_hunk_end && self.num_added != 0 || self.num_removed != 0 {
                    self.changed_lines
                        .push(self.num_added + self.num_context + 1);
                }
                self.num_context += 1;
            }
        }
    }

    pub fn is_line_changed_within(&self, start: u32, end: u32) -> bool {
        for line in &self.changed_lines {
            if *line > start && *line < end {
                return true;
            }
        }
        false
    }
}

pub enum Line {
    Add,
    Remove,
    Context,
}
