//! The core logic is as follows:
//!
//! 1. Parse the file to determine if it contains a OnChange block. This is done
//!    by processing the file line-by-line and checking against a regex.
//! 2. If a block is found, parse the block name. If no name is provided, use :default.
//! 3. Keep parsing until a ThenChange line is found (also using regex). If EOF is
//!    reached, return an error and terminate.
//! 4. A file can have mutliple blocks. So, a File struct contains a map of Blocks.
//! 5. Each block can optionally point to another Block. This is resolved eagerly by
//!    following the link to the block.
//!
//! F1 -> [B1, B2, ..., BN]
//!             |
//! F2 -> [B1, B2, ..., BN]
use std::cell::OnceCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::BufRead;
use std::path::{Path, PathBuf};

use anyhow::Result;
use ignore::WalkBuilder;
use regex::Regex;

use crate::git::{Hunk, Line};

thread_local! {
    // TODO(aksiksi): Clean these patterns up by making them more specific.
    static ON_CHANGE_PAT: OnceCell<Regex> = OnceCell::from(Regex::new(r".*LINT\.OnChange\((.*)\).*$").unwrap());
    static THEN_CHANGE_PAT: OnceCell<Regex> = OnceCell::from(Regex::new(r".*LINT\.ThenChange\((.*)\).*$").unwrap());
}

#[derive(Clone, Debug)]
pub struct ThenChangeTarget {
    block: String,
    file: Option<PathBuf>,
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
    // The name would be None for an untargetable block.
    name: Option<String>,
    start_line: u32,
    end_line: u32,
    then_change: ThenChange,
}

impl OnChangeBlock {
    pub fn name(&self) -> &str {
        self.name.as_deref().unwrap_or("<unnamed>")
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
    pub fn get_then_change_targets_as_keys<'a, 'b>(
        &'a self,
        default_path: &'b Path,
    ) -> Box<dyn Iterator<Item = (&'b Path, Option<&'a str>)> + 'b>
    where
        'a: 'b,
    {
        match &self.then_change {
            ThenChange::NoTarget | ThenChange::Unset => Box::new(std::iter::empty()),
            ThenChange::FileTarget(path) => Box::new(std::iter::once((path.as_path(), None))),
            ThenChange::BlockTarget(targets) => {
                Box::new(targets.iter().map(move |t| match &t.file {
                    Some(path) => (path.as_path(), Some(t.block.as_str())),
                    None => (default_path, Some(t.block.as_str())),
                }))
            }
        }
    }
}

#[derive(Debug)]
pub struct File {
    blocks: Vec<OnChangeBlock>,
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
        path: &Path,
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
                    path.display(),
                ));
            }
            block_name_to_start_line.insert(block_name.clone(), line_num);
        }

        block_stack.push(OnChangeBlock {
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

    fn parse<P: AsRef<Path>, Q: AsRef<Path>>(
        path: P,
        root_path: Option<Q>,
    ) -> Result<(Self, HashSet<PathBuf>)> {
        let path = path.as_ref().canonicalize()?;
        let root_path = root_path.map(|p| p.as_ref().canonicalize().unwrap());

        // Set of files that need to be parsed based on OnChange targets seen in this file.
        let mut files_to_parse: HashSet<PathBuf> = HashSet::new();

        let f = std::fs::File::open(&path)?;
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
                    &path,
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

#[derive(Debug)]
pub struct FileSet {
    files: BTreeMap<PathBuf, File>,
    num_blocks: usize,
}

impl FileSet {
    fn validate_block_target(
        &self,
        path: &Path,
        block: &OnChangeBlock,
        target: &ThenChangeTarget,
        blocks: &HashMap<(&Path, &str), &OnChangeBlock>,
    ) -> Result<()> {
        let (target_block, target_file) = (&target.block, target.file.as_ref());
        if let Some(file) = target_file {
            let block_key = (file.as_ref(), target_block.as_str());
            if !blocks.contains_key(&block_key) {
                return Err(anyhow::anyhow!(
                    r#"block "{}" in file "{}" has invalid OnChange target "{}:{}" (line {})"#,
                    block.name(),
                    path.display(),
                    file.display(),
                    target_block,
                    block.end_line,
                ));
            }
        }
        Ok(())
    }

    /// Returns a map of all _targetable_ blocks in the file set.
    fn on_change_blocks(&self) -> HashMap<(&Path, &str), &OnChangeBlock> {
        let mut blocks = HashMap::with_capacity(self.num_blocks);
        for (path, file) in self.files.iter() {
            for block in file.blocks.iter() {
                if block.name.is_none() {
                    continue;
                }
                blocks.insert((path.as_path(), block.name()), block);
            }
        }
        blocks
    }

    fn validate(&self) -> Result<()> {
        let blocks = self.on_change_blocks();

        for (path, file) in &self.files {
            for block in &file.blocks {
                match &block.then_change {
                    ThenChange::NoTarget => {}
                    ThenChange::BlockTarget(target_blocks) => {
                        for target in target_blocks {
                            Self::validate_block_target(&self, path, block, target, &blocks)?;
                        }
                    }
                    ThenChange::FileTarget(target_path) => {
                        if !self.files.contains_key(target_path) {
                            return Err(anyhow::anyhow!(
                                r#"block "{}" in file "{}" has invalid ThenChange target "{}" (line {})"#,
                                block.name(),
                                path.display(),
                                target_path.display(),
                                block.end_line,
                            ));
                        }
                    }
                    ThenChange::Unset => {
                        return Err(anyhow::anyhow!(
                            r#"block "{}" in file "{}" has an unset OnChange target (line {})"#,
                            block.name(),
                            path.display(),
                            block.end_line,
                        ));
                    }
                }
            }
        }

        Ok(())
    }

    /// Parses a set of files, as well as (recursively) any files referenced by OnChange blocks in the
    /// given set of files.
    pub fn from_files<P: AsRef<Path>, Q: AsRef<Path>>(
        paths: impl Iterator<Item = P>,
        root_path: Q,
    ) -> Result<Self> {
        let root_path = root_path.as_ref();
        let mut files = BTreeMap::new();

        let mut file_stack: Vec<PathBuf> = paths
            .map(|p| {
                let path = p.as_ref();
                if path.exists() {
                    path.to_owned()
                } else {
                    root_path.join(path)
                }
            })
            .collect();

        while let Some(path) = file_stack.pop() {
            let (file, files_to_parse) = File::parse(&path, Some(root_path))?;
            files.insert(path, file);
            for file_path in files_to_parse {
                if !files.contains_key(&file_path) {
                    file_stack.push(file_path);
                }
            }
        }

        let mut num_blocks = 0;
        for file in files.values() {
            num_blocks += file.blocks.len();
        }

        let file_set = FileSet { files, num_blocks };
        file_set.validate()?;
        Ok(file_set)
    }

    /// Recursively walks through all files in the given path and parses them.
    ///
    /// Note that this method respects .gitignore and .ignore files (via [[ignore]]).
    pub fn from_directory<P: AsRef<Path>>(path: P) -> Result<Self> {
        let root_path = path.as_ref().canonicalize()?;
        let mut files = BTreeMap::new();
        let mut file_stack: Vec<PathBuf> = Vec::new();

        if !root_path.is_dir() {
            return Err(anyhow::anyhow!(
                "{} is not a directory",
                root_path.display()
            ));
        }

        let dir_walker = WalkBuilder::new(&root_path).build();
        for entry in dir_walker {
            match entry {
                Err(e) => {
                    println!("Error: {}", e);
                    continue;
                }
                Ok(entry) => {
                    let path = entry.path();
                    if path.is_dir() {
                        continue;
                    }
                    let file_path = path.to_owned().canonicalize()?;
                    if !files.contains_key(&file_path) {
                        file_stack.push(file_path);
                    }
                }
            }
        }

        while let Some(path) = file_stack.pop() {
            let (file, files_to_parse) = File::parse(&path, Some(root_path.as_path()))?;
            files.insert(path, file);
            for file_path in files_to_parse {
                if !files.contains_key(&file_path) {
                    file_stack.push(file_path);
                }
            }
        }

        let mut num_blocks = 0;
        for file in files.values() {
            num_blocks += file.blocks.len();
        }

        let file_set = FileSet { files, num_blocks };
        file_set.validate()?;
        Ok(file_set)
    }

    /// Returns a iterator over all of the blocks in a specific file.
    pub fn on_change_blocks_in_file<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Option<impl Iterator<Item = &OnChangeBlock>> {
        self.files.get(path.as_ref()).map(|file| file.blocks.iter())
    }

    /// Returns a iterator over all of the blocks in a specific file.
    pub fn get_block_in_file<P: AsRef<Path>>(
        &self,
        path: P,
        block_name: &str,
    ) -> Option<&OnChangeBlock> {
        self.files
            .get(path.as_ref())
            .and_then(|f| f.blocks.iter().find(|b| b.name() == block_name))
    }

    pub fn files(&self) -> Vec<&Path> {
        self.files.keys().map(|p| p.as_path()).collect()
    }
}

#[cfg(test)]
mod test {
    use crate::test_helpers::TestDir;

    use super::*;

    #[test]
    fn test_from_directory() {
        let files = &[
            (
                "f1.txt",
                "LINT.OnChange(default)\n
                 abdbbda\nadadd\n
                 LINT.ThenChange(f2.txt:other)",
            ),
            (
                "f2.txt",
                "LINT.OnChange(other)\n
                 LINT.ThenChange(f1.txt:default)",
            ),
        ];
        let d = TestDir::from_files(files);
        FileSet::from_directory(d.path()).unwrap();
    }

    #[test]
    fn test_from_from_files() {
        let files = &[
            (
                "f1.txt",
                "LINT.OnChange()\n
                 abdbbda\n
                 adadd\n
                 LINT.ThenChange(f2.txt:other)",
            ),
            (
                "f2.txt",
                "LINT.OnChange(other)\n
                 LINT.ThenChange()",
            ),
        ];
        let d = TestDir::from_files(files);
        let file_names = files.iter().map(|f| f.0);
        FileSet::from_files(file_names, d.path()).unwrap();
    }

    #[test]
    fn test_from_from_files_file_target() {
        let files = &[
            (
                "f1.txt",
                "LINT.OnChange()\n
                 abdbbda\nadadd\n
                 LINT.ThenChange(f2.txt:other)",
            ),
            (
                "f2.txt",
                "LINT.OnChange(other)\n
                 LINT.ThenChange()",
            ),
            (
                "f3.txt",
                "LINT.OnChange(this)\n
                 LINT.ThenChange(f1.txt)",
            ),
        ];
        let d = TestDir::from_files(files);
        let file_names = files.iter().map(|f| f.0);
        FileSet::from_files(file_names, d.path()).unwrap();
    }

    #[test]
    fn test_from_files_relative_target() {
        let files = &[
            (
                "abc/f1.txt",
                "LINT.OnChange()\n
                 abdbbda\nadadd\n
                 LINT.ThenChange(../f2.txt:other)",
            ),
            (
                "f2.txt",
                "LINT.OnChange(other)\n
                 LINT.ThenChange()",
            ),
        ];
        let d = TestDir::from_files(files);
        let file_names = files.iter().map(|f| f.0);
        FileSet::from_files(file_names, d.path()).unwrap();
    }

    #[test]
    fn test_from_files_invalid_block_target_file_path() {
        let files = &[
            (
                "f1.txt",
                "LINT.OnChange(default)\n
                 abdbbda\nadadd\n
                 LINT.ThenChange(f3.txt:default)",
            ),
            (
                "f2.txt",
                "LINT.OnChange(default)\n
                 LINT.ThenChange(f1.txt:default)",
            ),
        ];
        let d = TestDir::from_files(files);
        let file_names = files.iter().map(|f| f.0);
        let res = FileSet::from_files(file_names, d.path());
        assert!(res.is_err());
        let err = res.unwrap_err().to_string();
        assert_eq!(
            err,
            r#"OnChange target file "f3.txt" on line 6 does not exist"#
        );
    }

    #[test]
    fn test_from_files_invalid_block_target() {
        let files = &[
            (
                "f1.txt",
                "LINT.OnChange(default)\n
                 abdbbda\nadadd\n
                 LINT.ThenChange(f2.txt:invalid)",
            ),
            (
                "f2.txt",
                "LINT.OnChange(default)\n
                 LINT.ThenChange(f1.txt:default)",
            ),
        ];
        let d = TestDir::from_files(files);
        let file_names = files.iter().map(|f| f.0);
        let res = FileSet::from_files(file_names, d.path());
        assert!(res.is_err());
        let err = res.unwrap_err().to_string();
        assert!(err.contains("has invalid OnChange target"));
    }

    #[test]
    fn test_from_files_duplicate_block_in_file() {
        let files = &[(
            "f1.txt",
            "LINT.OnChange(default)\n
             abdbbda\nadadd\n
             LINT.ThenChange(:other)
             LINT.OnChange(default)\n
             abdbbda\n
             LINT.ThenChange(:other)
             LINT.OnChange(other)\n
             abdbbda\nadadd\n
             LINT.ThenChange(:default)",
        )];
        let d = TestDir::from_files(files);
        let file_names = files.iter().map(|f| f.0);
        let res = FileSet::from_files(file_names, d.path());
        assert!(res.is_err());
        let err = res.unwrap_err().to_string();
        assert!(err.contains(r#"duplicate block name "default" found on lines 1 and 7"#));
    }

    #[test]
    fn test_from_files_nested_on_change() {
        let files = &[(
            "f1.txt",
            "LINT.OnChange(default)\n
             abdbbda\nadadd\n
             LINT.OnChange(other)\n
             LINT.ThenChange(:other)\n
             LINT.ThenChange()",
        )];
        let d = TestDir::from_files(files);
        let file_names = files.iter().map(|f| f.0);
        FileSet::from_files(file_names, d.path()).unwrap();
    }

    #[test]
    fn test_from_files_nested_on_change_unbalanced() {
        let files = &[(
            "f1.txt",
            "LINT.OnChange(default)\n
             abdbbda\nadadd\n
             LINT.OnChange(other)\n
             LINT.ThenChange(:other)",
        )];
        let d = TestDir::from_files(files);
        let file_names = files.iter().map(|f| f.0);
        let res = FileSet::from_files(file_names, d.path());
        assert!(res.is_err());
        let err = res.unwrap_err().to_string();
        assert!(err.contains(
            "while looking for ThenChange for block \"default\" which started on line 1"
        ));
    }

    #[test]
    fn test_from_files_eof_in_block() {
        let files = &[("f1.txt", "LINT.OnChange(default)\nabdbbda\nadadd\n")];
        let d = TestDir::from_files(files);
        let file_names = files.iter().map(|f| f.0);
        let res = FileSet::from_files(file_names, d.path());
        assert!(res.is_err());
        let err = res.unwrap_err().to_string();
        assert!(err.contains("reached end of file"));
    }
}
