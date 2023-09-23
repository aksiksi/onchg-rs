use std::collections::{hash_map::Entry, HashMap, HashSet};
use std::io::BufRead;
use std::path::{Path, PathBuf};

use anyhow::Result;
use regex::Regex;

/// The core logic is as follows:
///
/// 1. Parse the file to determine if it contains a OnChange block. This is done
///    by processing the file line-by-line and checking against a regex.
/// 2. If a block is found, parse the block name. If no name is provided, use :default.
/// 3. Keep parsing until a ThenChange line is found (also using regex). If EOF is
///    reached, return an error and terminate.
/// 4. A file can have mutliple blocks. So, a File struct contains a map of Blocks.
/// 5. Each block can optionally point to another Block. This is resolved eagerly by
///    following the link to the block.
///
/// F1 -> [B1, B2, ..., BN]
///             |
/// F2 -> [B1, B2, ..., BN]

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
    path: PathBuf,
    blocks: HashMap<String, Block>,
}

impl File {
    fn parse<P: AsRef<Path>, Q: AsRef<Path>>(
        path: P,
        files: &HashMap<PathBuf, File>,
        root_path: Option<Q>,
    ) -> Result<(Self, HashSet<PathBuf>)> {
        enum ParseState {
            Searching,
            InBlock,
        }
        let path = path.as_ref();

        let on_change_pat = Regex::new(r".*OnChange\((.*)\).*$").unwrap();
        let then_change_pat = Regex::new(r".*ThenChange\((.*)\).*$").unwrap();

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
                    println!("Line: {}", line);
                    // Try to parse the line as a OnChange block.
                    // TODO(aksiksi): OnChange should allow empty OR :<name>.
                    if let Some(captures) = on_change_pat.captures(&line) {
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

                    let captures = then_change_pat.captures(&line);
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
                            println!("Split target: {:?}", split_target);

                            // Block target in another file.
                            let file_name = split_target[0].to_string();
                            let mut file_path = PathBuf::from(&file_name);

                            if files.get(&file_path).is_none() {
                                if !file_path.exists() {
                                    if let Some(root_path) = root_path.as_ref() {
                                        // Try to join the path with the repo root.
                                        let root_path = root_path.as_ref();
                                        if root_path.exists() {
                                            file_path = root_path.join(file_path);
                                        }
                                    } else {
                                        // TODO: Otherwise, assume it's a relative path.
                                    }
                                }
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
                }
            }
        }

        match state {
            ParseState::Searching => {
                let f = File {
                    path: path.to_owned(),
                    blocks,
                };
                Ok((f, files_to_parse))
            }
            _ => Err(anyhow::anyhow!(
                "unexpected EOF while looking for ThenChange for block {}",
                block_name.unwrap(),
            )),
        }
    }
}

#[derive(Debug)]
pub struct FileSet {
    files: HashMap<PathBuf, File>,
}

impl FileSet {
    pub fn parse_staged_files<P: AsRef<Path>>(path: P, is_repo_path: bool) -> Result<Self> {
        let (staged_files, repo_path) =
            super::git::get_staged_file_paths(path, is_repo_path).unwrap();
        // Strip .git folder from path.
        let root_path = repo_path.parent().unwrap();

        let mut files = HashMap::new();
        let mut file_stack: Vec<PathBuf> = staged_files.clone();

        while let Some(f) = file_stack.pop() {
            let (file, to_parse) = File::parse(f, &mut files, Some(root_path))?;
            files.insert(file.path.clone(), file);
            for file_path in to_parse {
                file_stack.push(file_path);
            }
        }

        Ok(FileSet { files })
    }

    pub fn blocks(&self) -> HashMap<(&PathBuf, &str), &Block> {
        let mut blocks = HashMap::new();
        for (path, file) in self.files.iter() {
            for (name, block) in file.blocks.iter() {
                blocks.insert((path, name.as_str()), block);
            }
        }
        blocks
    }
}
