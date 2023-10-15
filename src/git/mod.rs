use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::Result;

pub mod cli;
#[cfg(feature = "git")]
mod lib;

pub trait Repo {
    fn get_staged_files(&self) -> Result<Vec<PathBuf>>;
    // NOTE: We could optimize by having it accept a list of files to check.
    fn get_staged_hunks(&self) -> Result<BTreeMap<PathBuf, Vec<Hunk>>>;
}

#[derive(Debug)]
pub struct Hunk {
    /// Start line of this hunk in the _new_ file.
    pub start_line: u32,
    /// End line of this hunk in the _new_ file.
    pub end_line: u32,
    /// Lines that have changed in this hunk.
    pub lines: Vec<Line>,
}

#[derive(Debug)]
pub enum Line {
    /// Added line number (in _new_ file).
    Add(u32),
    /// Removed line number (in _old_ file).
    Remove(u32),
    /// Context line number (old, new).
    Context(u32, u32),
}

impl Hunk {
    pub fn is_block_overlap(&self, start: u32, end: u32) -> bool {
        // Block contains hunk.
        self.start_line >= start && self.end_line <= end ||
        // Hunk contains block.
        start >= self.start_line && end <= self.end_line ||
        // Hunk starts before block and ends within it.
        start >= self.start_line && self.end_line <= end ||
        // Hunk starts after block and ends after it.
        self.start_line >= start && self.end_line >= end
    }
}
