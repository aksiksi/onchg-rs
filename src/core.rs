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
use std::collections::{hash_map::Entry, BTreeMap, HashMap, HashSet};
use std::io::BufRead;
use std::path::{Path, PathBuf};

use anyhow::Result;
use ignore::WalkBuilder;
use regex::Regex;

const DEFAULT_ON_CHANGE_BLOCK_NAME: &str = ":default";

thread_local! {
    // TODO(aksiksi): Clean these patterns up by making them more specific.
    static ON_CHANGE_PAT: OnceCell<Regex> = OnceCell::from(Regex::new(r".*OnChange\((.*)\).*$").unwrap());
    static THEN_CHANGE_PAT: OnceCell<Regex> = OnceCell::from(Regex::new(r".*ThenChange\((.*)\).*$").unwrap());
}

#[derive(Clone, Debug)]
pub enum ThenChange {
    Unset,
    None,
    Block {
        block: String,
        file: Option<PathBuf>,
    },
}

#[derive(Clone, Debug)]
pub struct OnChangeBlock {
    pub name: String,
    pub start_line: u32,
    pub end_line: u32,
    pub then_change: ThenChange,
}

#[derive(Debug)]
pub struct File {
    blocks: HashMap<String, OnChangeBlock>,
}

impl File {
    fn build_then_change(
        files_to_parse: &mut HashSet<PathBuf>,
        line_num: usize,
        path: &Path,
        root_path: Option<&Path>,
        then_change_target: &str,
    ) -> Result<ThenChange> {
        let then_change_target = then_change_target.trim();
        if then_change_target.is_empty() {
            return Ok(ThenChange::None);
        }

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
            return Ok(ThenChange::Block {
                block: block_name.to_string(),
                file: None,
            });
        }

        // Block target in another file.
        let mut file_path = PathBuf::from(split_target[0]);
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
            if root_with_path.is_some() && root_with_path.as_ref().unwrap().exists() {
                file_path = root_with_path.unwrap();
            } else if relative_path.exists() {
                // Otherwise, assume it's a relative path.
                file_path = relative_path;
            } else {
                return Err(anyhow::anyhow!(
                    r#"OnChange target file "{}" does not exist on line {}"#,
                    file_path.display(),
                    line_num
                ));
            }
        }
        file_path = file_path.canonicalize().unwrap();
        files_to_parse.insert(file_path.clone());

        return Ok(ThenChange::Block {
            block: block_name.to_string(),
            file: Some(file_path),
        });
    }

    fn parse<P: AsRef<Path>, Q: AsRef<Path>>(
        path: P,
        root_path: Option<Q>,
    ) -> Result<(Self, HashSet<PathBuf>)> {
        let path = path.as_ref().canonicalize()?;
        let root_path = root_path.map(|p| p.as_ref().canonicalize().unwrap());

        let mut blocks: HashMap<String, OnChangeBlock> = HashMap::new();
        // Set of files that need to be parsed based on OnChange targets seen in this file.
        let mut files_to_parse: HashSet<PathBuf> = HashSet::new();

        let f = std::fs::File::open(&path)?;
        let reader = std::io::BufReader::new(f);

        enum ParseState {
            Searching,
            InBlock,
        }
        let mut state = ParseState::Searching;
        let mut block_name: Option<String> = None;
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
            match state {
                ParseState::Searching => {
                    // Try to parse the line as a OnChange.
                    // TODO(aksiksi): OnChange should allow empty OR :<name>.
                    let captures = ON_CHANGE_PAT.with(|c| c.get().unwrap().captures(&line));

                    if let Some(captures) = captures {
                        let mut parsed = String::from(captures.get(1).unwrap().as_str().trim());
                        if parsed.is_empty() {
                            // Unnamed OnChange block.
                            parsed = String::from(DEFAULT_ON_CHANGE_BLOCK_NAME);
                        }
                        if let Some(p) = parsed.strip_prefix(":") {
                            parsed = p.to_string();
                        } else {
                            return Err(anyhow::anyhow!(
                                "OnChange block name does not start with \":\" on line {}: {}",
                                line_num,
                                parsed
                            ));
                        }
                        block_name = Some(parsed.to_string());
                    } else {
                        continue;
                    }

                    match blocks.entry(block_name.clone().unwrap()) {
                        Entry::Occupied(e) => {
                            let block = e.get();
                            return Err(anyhow::anyhow!(
                                "duplicate block names found on lines {} and {}: {}",
                                block.start_line,
                                line_num,
                                block.name
                            ));
                        }
                        Entry::Vacant(e) => {
                            let block = OnChangeBlock {
                                name: block_name.clone().unwrap(),
                                start_line: line_num as u32,
                                end_line: 0,
                                then_change: ThenChange::Unset,
                            };
                            e.insert(block);
                        }
                    }
                    state = ParseState::InBlock;
                }
                ParseState::InBlock => {
                    // Try to parse the line as a ThenChange.
                    let block_name = block_name.as_ref();

                    let captures = THEN_CHANGE_PAT.with(|c| c.get().unwrap().captures(&line));
                    if captures.is_none() {
                        continue;
                    }

                    // We have a valid OnChange.
                    let captures = captures.unwrap();

                    if block_name.is_none() {
                        return Err(anyhow::anyhow!(
                            "ThenChange found before OnChange on line {}",
                            line_num
                        ));
                    }

                    let block_name = block_name.unwrap();
                    if !blocks.contains_key(block_name) {
                        return Err(anyhow::anyhow!(
                            "block {} does not exist, but found ThenChange on line {}",
                            block_name,
                            line_num
                        ));
                    }

                    let block_target = Self::build_then_change(
                        &mut files_to_parse,
                        line_num,
                        &path,
                        root_path.as_ref().map(|p| p.as_path()),
                        captures.get(1).unwrap().as_str(),
                    )?;

                    let block = blocks.get(block_name).unwrap().clone();
                    blocks.insert(
                        block.name.clone(),
                        OnChangeBlock {
                            end_line: line_num as u32,
                            then_change: block_target,
                            ..block
                        },
                    );

                    state = ParseState::Searching;
                }
            }
        }

        match state {
            ParseState::Searching => Ok((File { blocks }, files_to_parse)),
            // If we've hit EOF but are not currently searching for a block, it means
            // we have an unclosed block.
            _ => Err(anyhow::anyhow!(
                "unexpected EOF in {} while looking for ThenChange for block \":{}\"",
                path.display(),
                block_name.unwrap(),
            )),
        }
    }
}

#[derive(Debug)]
pub struct FileSet {
    files: BTreeMap<PathBuf, File>,
    num_blocks: usize,
}

impl FileSet {
    fn validate(&self) -> Result<()> {
        let blocks = self.on_change_blocks();

        for (path, file) in &self.files {
            for (name, block) in &file.blocks {
                match &block.then_change {
                    ThenChange::None => {}
                    ThenChange::Block { block, file } => {
                        if let Some(file) = file {
                            let block_key = (file.as_ref(), block.as_str());
                            if !blocks.contains_key(&block_key) {
                                return Err(anyhow::anyhow!(
                                    r#"block "{}" in file "{}" has invalid OnChange target "{}:{}""#,
                                    name,
                                    path.display(),
                                    file.display(),
                                    block
                                ));
                            }
                        }
                    }
                    ThenChange::Unset => {
                        return Err(anyhow::anyhow!(
                            r#"block "{}" in file "{}" has an invalid OnChange target"#,
                            block.name,
                            path.display()
                        ));
                    }
                }
            }
        }

        Ok(())
    }

    /// Parses a set of files, as well as (recursively) any files referenced by OnChange blocks in the
    /// given set of files.
    pub fn from_files<P: AsRef<Path>, Q: AsRef<Path>>(paths: &[P], root_path: Q) -> Result<Self> {
        let root_path = root_path.as_ref();
        let mut files = BTreeMap::new();

        let mut file_stack: Vec<PathBuf> = paths
            .iter()
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
    #[allow(unused)]
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

    /// Returns a map of all blocks in the file set.
    pub fn on_change_blocks(&self) -> HashMap<(&Path, &str), &OnChangeBlock> {
        let mut blocks = HashMap::with_capacity(self.num_blocks);
        for (path, file) in self.files.iter() {
            for (name, block) in file.blocks.iter() {
                blocks.insert((path.as_path(), name.as_str()), block);
            }
        }
        blocks
    }

    /// Returns a map of all blocks in the file set.
    pub fn on_change_blocks_in_file<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Option<HashMap<&str, &OnChangeBlock>> {
        match self.files.get(path.as_ref()) {
            None => None,
            Some(file) => {
                let mut blocks = HashMap::with_capacity(file.blocks.len());
                for (name, block) in file.blocks.iter() {
                    blocks.insert(name.as_str(), block);
                }
                Some(blocks)
            }
        }
    }

    pub fn get_on_change_block<P: AsRef<Path>>(
        &self,
        path: P,
        block_name: &str,
    ) -> Option<&OnChangeBlock> {
        match self.files.get(path.as_ref()) {
            None => None,
            Some(file) => file.blocks.get(block_name),
        }
    }

    pub fn files(&self) -> Vec<&Path> {
        self.files.keys().map(|p| p.as_path()).collect()
    }
}

#[cfg(test)]
mod test {
    use crate::helpers::TestDir;

    use super::*;

    #[test]
    fn test_from_directory() {
        let files = &[
            (
                "f1.txt",
                "OnChange()\nabdbbda\nadadd\nThenChange(f2.txt:default)",
            ),
            ("f2.txt", "OnChange()\nThenChange(f1.txt:default)"),
        ];
        let d = TestDir::from_files(files).unwrap();
        FileSet::from_directory(d.path()).unwrap();
    }

    #[test]
    fn test_from_from_files() {
        let files = &[
            (
                "f1.txt",
                "OnChange()\nabdbbda\nadadd\nThenChange(f2.txt:default)",
            ),
            ("f2.txt", "OnChange()\nThenChange(f1.txt:default)"),
        ];
        let d = TestDir::from_files(files).unwrap();
        let file_names = files.iter().map(|f| f.0).collect::<Vec<_>>();
        FileSet::from_files(&file_names, d.path()).unwrap();
    }

    #[test]
    fn test_from_files_invalid_block_target_file_path() {
        let files = &[
            (
                "f1.txt",
                "OnChange()\nabdbbda\nadadd\nThenChange(f3.txt:default)",
            ),
            ("f2.txt", "OnChange()\nThenChange(f1.txt:default)"),
        ];
        let d = TestDir::from_files(files).unwrap();
        let file_names = files.iter().map(|f| f.0).collect::<Vec<_>>();
        let res = FileSet::from_files(&file_names, d.path());
        assert!(res.is_err());
        let err = res.unwrap_err().to_string();
        assert_eq!(
            err,
            r#"OnChange target file "f3.txt" does not exist on line 4"#
        );
    }

    #[test]
    fn test_from_files_invalid_block_target() {
        let files = &[
            (
                "f1.txt",
                "OnChange()\nabdbbda\nadadd\nThenChange(f2.txt:invalid)",
            ),
            ("f2.txt", "OnChange()\nThenChange(f1.txt:default)"),
        ];
        let d = TestDir::from_files(files).unwrap();
        let file_names = files.iter().map(|f| f.0).collect::<Vec<_>>();
        let res = FileSet::from_files(&file_names, d.path());
        assert!(res.is_err());
        let err = res.unwrap_err().to_string();
        assert!(err.contains("has invalid OnChange target"));
    }
}
