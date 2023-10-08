use std::cell::OnceCell;
use std::collections::{HashMap, HashSet};
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use anyhow::Result;
use regex::Regex;

use crate::git::{Hunk, Line};

thread_local! {
    // TODO(aksiksi): Clean these patterns up by making them more specific.
    static ON_CHANGE_PAT: OnceCell<Regex> = OnceCell::from(Regex::new(r".*LINT\.OnChange\((.*)\).*$").unwrap());
    static THEN_CHANGE_PAT: OnceCell<Regex> = OnceCell::from(Regex::new(r".*LINT\.ThenChange\((.*)\).*$").unwrap());
}

#[derive(Clone, Debug)]
pub struct ThenChangeTarget {
    pub block: String,
    pub file: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub enum ThenChange {
    Unset,
    NoTarget,
    /// Entire file.
    FileTarget(PathBuf),
    /// One or more blocks.
    BlockTarget(Vec<ThenChangeTarget>),
}

#[derive(Clone, Debug)]
pub struct OnChangeBlock {
    file: Rc<PathBuf>,
    // The name would be None for an untargetable block.
    name: Option<String>,
    start_line: u32,
    end_line: u32,
    then_change: ThenChange,
}

impl OnChangeBlock {
    pub fn new(
        file: PathBuf,
        name: Option<String>,
        start_line: u32,
        end_line: u32,
        then_change: ThenChange,
    ) -> Self {
        Self {
            file: Rc::new(file),
            name,
            start_line,
            end_line,
            then_change,
        }
    }

    pub fn name(&self) -> &str {
        self.name.as_deref().unwrap_or("<unnamed>")
    }

    pub fn file(&self) -> &Path {
        &self.file
    }

    pub fn name_raw(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn is_targetable(&self) -> bool {
        self.name.is_some()
    }

    pub fn start_line(&self) -> u32 {
        self.start_line
    }

    pub fn end_line(&self) -> u32 {
        self.end_line
    }

    pub fn then_change(&self) -> &ThenChange {
        &self.then_change
    }

    pub fn is_changed_by_hunk(&self, hunk: &Hunk) -> bool {
        let mut old_start_line = None;
        let mut old_end_line = None;
        let mut lines_removed = Vec::new();
        for line in &hunk.lines {
            match *line {
                Line::Add(l) => {
                    // A line was added inside the block.
                    if l >= self.start_line && l <= self.end_line {
                        return true;
                    }
                }
                Line::Remove(l) => {
                    // Keep track of removed blocks to check against the old
                    // block lines.
                    lines_removed.push(l);
                }
                Line::Context(old, new) => {
                    // Check if this context line is a start or end line for the block.
                    //
                    // Note that we expect _at least_ one of the context lines to be either
                    // a start or end line. If a block start/end is removed, the block is
                    // invalid. If it was removed and re-added, it will be picked up as
                    // an added line.
                    if self.start_line == new {
                        old_start_line = Some(old);
                    } else if self.end_line == new {
                        old_end_line = Some(old);
                    }
                }
            }
        }

        // Check each of the removed lines against the old block start or end lines.
        // This is how we detect if a line was removed inside a block.
        for l in lines_removed {
            match (old_start_line, old_end_line) {
                // Removed line falls between the (old) start and end lines of the block.
                (Some(old_start_line), Some(old_end_line))
                    if l >= old_start_line && l <= old_end_line =>
                {
                    return true;
                }
                // Removed line is after the (old) start line of the block.
                (Some(old_start_line), None) if l >= old_start_line => {
                    return true;
                }
                // Removed line is before the (old) end line of the block.
                (None, Some(old_end_line)) if l <= old_end_line => {
                    return true;
                }
                _ => (),
            }
        }

        false
    }

    /// Returns an iterator over ThenChangeTarget(s) as tuples of (file_path, block_name).
    /// If a target has no path set, it will be replaced with the provided file path.
    pub fn get_then_change_targets_as_keys<'a>(
        &'a self,
    ) -> Box<dyn Iterator<Item = (&Path, Option<&str>)> + 'a> {
        match &self.then_change {
            ThenChange::NoTarget | ThenChange::Unset => Box::new(std::iter::empty()),
            ThenChange::FileTarget(path) => Box::new(std::iter::once((path.as_path(), None))),
            ThenChange::BlockTarget(targets) => {
                Box::new(targets.iter().map(move |t| match &t.file {
                    Some(path) => (path.as_path(), Some(t.block.as_str())),
                    None => (self.file.as_path(), Some(t.block.as_str())),
                }))
            }
        }
    }
}

#[derive(Debug)]
pub struct File {
    pub(crate) blocks: Vec<OnChangeBlock>,
}

impl File {
    fn parse_then_target_file_path(
        line_num: usize,
        path: &Path,
        root_path: Option<&Path>,
        then_change_target: &str,
    ) -> Result<PathBuf> {
        let mut file_path = PathBuf::from(then_change_target);

        if !file_path.exists() {
            let root_with_path = if let Some(root_path) = root_path {
                if root_path.exists() {
                    Some(root_path.join(&file_path))
                } else {
                    None
                }
            } else {
                None
            };
            let relative_path = path.parent().unwrap().join(&file_path);

            // Always prioritize the relative path case (i.e., targets in the same directory).
            if relative_path.exists() {
                file_path = relative_path;
            } else if root_with_path.is_some() && root_with_path.as_ref().unwrap().exists() {
                file_path = root_with_path.unwrap();
            } else {
                return Err(anyhow::anyhow!(
                    r#"OnChange target file "{}" on line {} does not exist"#,
                    file_path.display(),
                    line_num
                ));
            }
        }

        Ok(file_path.canonicalize().unwrap())
    }

    fn parse_single_then_change_target(
        line_num: usize,
        path: &Path,
        root_path: Option<&Path>,
        then_change_target: &str,
    ) -> Result<ThenChangeTarget> {
        let split_target: Vec<&str> = then_change_target.split(":").collect();
        if split_target.len() < 2 {
            return Err(anyhow::anyhow!(
                "invalid ThenChange target on line {}: {}",
                line_num,
                then_change_target
            ));
        }
        let block_name = split_target[1];
        if split_target[0] == "" {
            // Block target in same file.
            return Ok(ThenChangeTarget {
                block: block_name.to_string(),
                file: None,
            });
        }

        // Block target in another file.
        let file_path =
            Self::parse_then_target_file_path(line_num, path, root_path, split_target[0])?;

        Ok(ThenChangeTarget {
            block: block_name.to_string(),
            file: Some(file_path),
        })
    }

    fn build_then_change(
        files_to_parse: &mut HashSet<PathBuf>,
        line_num: usize,
        path: &Path,
        root_path: Option<&Path>,
        then_change_target: &str,
    ) -> Result<ThenChange> {
        let then_change_target = then_change_target.trim();
        if then_change_target.is_empty() {
            return Ok(ThenChange::NoTarget);
        }
        if !then_change_target.contains(":") {
            // Try to parse as just a file target.
            let file_path =
                Self::parse_then_target_file_path(line_num, path, root_path, then_change_target)?;
            files_to_parse.insert(file_path.clone());
            return Ok(ThenChange::FileTarget(file_path));
        }

        // Split on comma to build a list of targets.
        let mut then_change_targets = Vec::new();
        let split_by_comma: Vec<&str> = then_change_target.split(",").collect();
        let split_by_comma = if split_by_comma.len() == 0 {
            // Single target.
            vec![then_change_target]
        } else {
            split_by_comma
        };

        for target in split_by_comma {
            let target = target.trim();
            let t = Self::parse_single_then_change_target(line_num, path, root_path, target)?;
            if let Some(f) = &t.file {
                files_to_parse.insert(f.clone());
            }
            then_change_targets.push(t);
        }

        Ok(ThenChange::BlockTarget(then_change_targets))
    }

    fn try_parse_on_change_line(line: &str) -> Option<String> {
        match ON_CHANGE_PAT.with(|c| c.get().unwrap().captures(&line)) {
            None => None,
            Some(captures) => Some(String::from(captures.get(1).unwrap().as_str().trim())),
        }
    }

    fn try_parse_then_change_line(line: &str) -> Option<String> {
        match THEN_CHANGE_PAT.with(|c| c.get().unwrap().captures(&line)) {
            None => None,
            Some(captures) => Some(String::from(captures.get(1).unwrap().as_str().trim())),
        }
    }

    fn handle_on_change(
        file: Rc<PathBuf>,
        parsed: String,
        line_num: usize,
        block_name_to_start_line: &mut HashMap<String, usize>,
        block_stack: &mut Vec<OnChangeBlock>,
    ) -> Result<()> {
        let block_name = if parsed.is_empty() {
            // An unnamed OnChange block is untargetable by other blocks.
            None
        } else {
            Some(parsed)
        };

        // Check for a duplicate block in the file.
        if let Some(block_name) = &block_name {
            if block_name_to_start_line.contains_key(block_name) {
                return Err(anyhow::anyhow!(
                    "duplicate block name \"{}\" found on lines {} and {} of {}",
                    block_name,
                    block_name_to_start_line[block_name],
                    line_num,
                    file.display(),
                ));
            }
            block_name_to_start_line.insert(block_name.clone(), line_num);
        }

        block_stack.push(OnChangeBlock {
            file,
            name: block_name,
            start_line: line_num as u32,
            end_line: 0,
            then_change: ThenChange::Unset,
        });

        Ok(())
    }

    fn handle_then_change(
        path: &Path,
        root_path: Option<&Path>,
        parsed: &str,
        line_num: usize,
        files_to_parse: &mut HashSet<PathBuf>,
        block_stack: &mut Vec<OnChangeBlock>,
    ) -> Result<OnChangeBlock> {
        let mut block = if let Some(block) = block_stack.pop() {
            block
        } else {
            return Err(anyhow::anyhow!(
                r#"found ThenChange on line {} in "{}" with no matching OnChange"#,
                line_num,
                path.display(),
            ));
        };
        block.end_line = line_num as u32;
        block.then_change =
            Self::build_then_change(files_to_parse, line_num, &path, root_path, &parsed)?;
        Ok(block)
    }

    pub fn parse<P: AsRef<Path>>(
        path: Rc<PathBuf>,
        root_path: Option<P>,
    ) -> Result<(Self, HashSet<PathBuf>)> {
        let root_path = root_path.map(|p| p.as_ref().canonicalize().unwrap());

        // Set of files that need to be parsed based on OnChange targets seen in this file.
        let mut files_to_parse: HashSet<PathBuf> = HashSet::new();

        let f = std::fs::File::open(path.as_path())?;
        let reader = std::io::BufReader::new(f);

        let mut blocks: Vec<OnChangeBlock> = Vec::new();
        let mut block_stack: Vec<OnChangeBlock> = Vec::new();
        let mut block_name_to_start_line: HashMap<String, usize> = HashMap::new();

        for (line_num, line) in reader.lines().enumerate() {
            let line = match line {
                Ok(l) => l,
                Err(e) => {
                    // TODO(aksiksi): We can probably do something cleaner here.
                    eprintln!("Error reading lines from {}: {}", path.display(), e);
                    return Ok((File { blocks }, files_to_parse));
                }
            };
            let line_num = line_num + 1;
            if let Some(parsed) = Self::try_parse_on_change_line(&line) {
                Self::handle_on_change(
                    path.clone(),
                    parsed,
                    line_num,
                    &mut block_name_to_start_line,
                    &mut block_stack,
                )?;
            } else if let Some(parsed) = Self::try_parse_then_change_line(&line) {
                let block = Self::handle_then_change(
                    &path,
                    root_path.as_deref(),
                    &parsed,
                    line_num,
                    &mut files_to_parse,
                    &mut block_stack,
                )?;
                blocks.push(block);
            }
        }

        if block_stack.len() > 0 {
            // We've hit EOF with an unclosed OnChange block.
            let block = block_stack.last_mut().unwrap();
            return Err(anyhow::anyhow!(
                "reached end of file {} while looking for ThenChange for block \"{}\" which started on line {}",
                path.display(),
                block.name(),
                block.start_line,
            ));
        }

        Ok((File { blocks }, files_to_parse))
    }
}
