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

thread_local! {
    static ON_CHANGE_PAT: OnceCell<Regex> = OnceCell::from(Regex::new(r".*OnChange\((.*)\).*$").unwrap());
    static THEN_CHANGE_PAT: OnceCell<Regex> = OnceCell::from(Regex::new(r".*ThenChange\((.*)\).*$").unwrap());
}

#[derive(Clone, Debug)]
pub enum BlockTarget {
    Unset,
    None,
    Block {
        block: String,
        file: Option<PathBuf>,
    },
}

#[derive(Clone, Debug)]
pub struct Block {
    pub name: String,
    pub start_line: usize,
    pub end_line: usize,
    pub on_change: BlockTarget,
}

#[derive(Debug)]
pub struct File {
    blocks: HashMap<String, Block>,
}

impl File {
    fn parse<P: AsRef<Path>, Q: AsRef<Path>>(
        path: P,
        files: &BTreeMap<PathBuf, File>,
        root_path: Option<Q>,
    ) -> Result<(Self, HashSet<PathBuf>)> {
        enum ParseState {
            Searching,
            InBlock,
        }
        let path = path.as_ref();
        let root_path = root_path
            .as_ref()
            .map(|p| p.as_ref().canonicalize().unwrap());

        let mut blocks: HashMap<String, Block> = HashMap::new();
        let mut files_to_parse: HashSet<PathBuf> = HashSet::new();

        let f = std::fs::File::open(&path)?;
        let reader = std::io::BufReader::new(f);

        let mut state = ParseState::Searching;
        let mut block_name: Option<String> = None;
        for (i, line) in reader.lines().enumerate() {
            let line = line?;
            match state {
                ParseState::Searching => {
                    // Try to parse the line as a OnChange block.
                    // TODO(aksiksi): OnChange should allow empty OR :<name>.
                    let captures = ON_CHANGE_PAT.with(|c| c.get().unwrap().captures(&line));
                    if let Some(captures) = captures {
                        let mut parsed = String::from(captures.get(1).unwrap().as_str().trim());
                        if parsed.is_empty() {
                            // Unnamed OnChange block.
                            parsed = String::from("default");
                        }
                        if let Some(p) = parsed.strip_prefix(":") {
                            parsed = p.to_string();
                        }
                        block_name = Some(parsed.to_string());
                    } else {
                        continue;
                    }
                    match blocks.entry(block_name.clone().unwrap()) {
                        Entry::Occupied(e) => {
                            let block = e.get();
                            return Err(anyhow::anyhow!(
                                "duplicate block name found on line {}: {}",
                                block.start_line,
                                block.name
                            ));
                        }
                        Entry::Vacant(e) => {
                            let block = Block {
                                name: block_name.clone().unwrap(),
                                start_line: i + 1,
                                end_line: 0,
                                on_change: BlockTarget::Unset,
                            };
                            e.insert(block);
                        }
                    }
                    state = ParseState::InBlock;
                }
                ParseState::InBlock => {
                    // Try to parse the line as a OnChange block.
                    let block_name = block_name.as_ref();

                    let captures = THEN_CHANGE_PAT.with(|c| c.get().unwrap().captures(&line));
                    if captures.is_none() {
                        continue;
                    }
                    let captures = captures.unwrap();

                    if block_name.is_none() {
                        return Err(anyhow::anyhow!(
                            "ThenChange found before OnChange on line {}",
                            i + 1
                        ));
                    }
                    let block_name = block_name.unwrap();
                    if !blocks.contains_key(block_name) {
                        return Err(anyhow::anyhow!(
                            "block {} does not exist, but found ThenChange on line {}",
                            block_name,
                            i + 1
                        ));
                    }

                    let block_target;
                    let parsed = String::from(captures.get(1).unwrap().as_str().trim());
                    if parsed.is_empty() {
                        block_target = BlockTarget::None;
                    } else {
                        let split_target: Vec<&str> = parsed.split(":").collect();
                        if split_target.len() < 2 {
                            return Err(anyhow::anyhow!(
                                "invalid ThenChange target on line {}: {}",
                                i + 1,
                                parsed
                            ));
                        }
                        if split_target[0] == "" {
                            // Block target in same file.
                            block_target = BlockTarget::Block {
                                block: split_target[1].to_string(),
                                file: None,
                            };
                        } else {
                            // Block target in another file.
                            let file_name = split_target[0].to_string();
                            let mut file_path = PathBuf::from(&file_name);

                            if files.get(&file_path).is_none() {
                                if !file_path.exists() {
                                    let root_with_path = if let Some(root_path) = root_path.as_ref()
                                    {
                                        if root_path.exists() {
                                            Some(root_path.join(&file_path))
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    };
                                    let relative_path = path.parent().unwrap().join(&file_path);
                                    if root_with_path.is_some()
                                        && root_with_path.as_ref().unwrap().exists()
                                    {
                                        file_path = root_with_path.unwrap();
                                    } else if relative_path.exists() {
                                        // Otherwise, assume it's a relative path.
                                        file_path = relative_path;
                                    } else {
                                        return Err(anyhow::anyhow!(
                                            "file {} does not exist on line {}",
                                            file_name,
                                            i + 1
                                        ));
                                    }
                                }
                                file_path = file_path.canonicalize().unwrap();
                                files_to_parse.insert(file_path.clone());
                            }

                            block_target = BlockTarget::Block {
                                block: split_target[1].to_string(),
                                file: Some(file_path),
                            };
                        }
                    }

                    let block = blocks.get(block_name).unwrap().to_owned();
                    blocks.insert(
                        block_name.clone(),
                        Block {
                            name: block_name.clone(),
                            start_line: block.start_line,
                            end_line: i + 1,
                            on_change: block_target,
                        },
                    );

                    state = ParseState::Searching;
                }
            }
        }

        match state {
            ParseState::Searching => Ok((File { blocks }, files_to_parse)),
            // If we reach EOF and are not searching for a block, the file is malformed.
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
    pub fn parse_staged_files<P: AsRef<Path>>(path: P, is_repo_path: bool) -> Result<Self> {
        let (staged_files, repo_path) =
            super::git::get_staged_file_paths(path, is_repo_path).unwrap();
        // Strip .git folder from path.
        let root_path = repo_path.parent().unwrap();

        let mut files = BTreeMap::new();
        let mut file_stack: Vec<PathBuf> = staged_files.clone();

        let mut num_blocks = 0;
        while let Some(path) = file_stack.pop() {
            let (file, files_to_parse) = File::parse(&path, &mut files, Some(root_path))?;
            num_blocks += file.blocks.len();
            files.insert(path, file);
            for file_path in files_to_parse {
                file_stack.push(file_path);
            }
        }

        Ok(FileSet { files, num_blocks })
    }

    pub fn parse<P: AsRef<Path>>(path: P) -> Result<Self> {
        let root_path = path.as_ref();
        let mut files = BTreeMap::new();
        let mut file_stack: Vec<PathBuf> = Vec::new();

        let dir_walker = WalkBuilder::new(root_path).build();

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
                    file_stack.push(path.to_owned().canonicalize().unwrap());
                }
            }
        }

        let mut num_blocks = 0;
        while let Some(path) = file_stack.pop() {
            let (file, files_to_parse) = File::parse(&path, &mut files, Some(root_path))?;
            num_blocks += file.blocks.len();
            files.insert(path, file);
            for file_path in files_to_parse {
                file_stack.push(file_path);
            }
        }

        Ok(FileSet { files, num_blocks })
    }

    pub fn blocks(&self) -> HashMap<(&Path, &str), &Block> {
        let mut blocks = HashMap::with_capacity(self.num_blocks);
        for (path, file) in self.files.iter() {
            for (name, block) in file.blocks.iter() {
                blocks.insert((path.as_path(), name.as_str()), block);
            }
        }
        blocks
    }
}
