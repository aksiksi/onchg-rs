use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use bstr::ByteSlice;
use regex::bytes::{Captures, Regex};

use crate::git::{Hunk, Line};

const ON_CHANGE_GROUP: &str = "on_change";
const THEN_CHANGE_GROUP: &str = "then_change";
pub const ON_CHANGE_PAT_STR: &str =
    r"LINT\.OnChange\((?<on_change>.*?)\)|LINT\.ThenChange\((?<then_change>.*?)\)";
lazy_static::lazy_static! {
    static ref ON_CHANGE_PAT: Regex = Regex::new(ON_CHANGE_PAT_STR).unwrap();
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
    file: Arc<PathBuf>,
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
            file: Arc::new(file),
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
enum LineMatch<'a> {
    OnChange(usize, &'a [u8]),
    ThenChange(usize, &'a [u8]),
}

impl<'a> LineMatch<'a> {
    #[inline(always)]
    fn pos(&self) -> usize {
        match *self {
            LineMatch::OnChange(p, _) | LineMatch::ThenChange(p, _) => p,
        }
    }

    #[inline(always)]
    fn data(&self) -> &[u8] {
        match *self {
            LineMatch::OnChange(_, d) | LineMatch::ThenChange(_, d) => d,
        }
    }
}

#[derive(Debug)]
pub struct File {
    pub(crate) path: PathBuf,
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

    fn handle_on_change(
        file: Arc<PathBuf>,
        parsed: &str,
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
        if let Some(block_name) = block_name {
            if block_name_to_start_line.contains_key(block_name) {
                return Err(anyhow::anyhow!(
                    "duplicate block name \"{}\" found on lines {} and {} of {}",
                    block_name,
                    block_name_to_start_line[block_name],
                    line_num,
                    file.display(),
                ));
            }
            block_name_to_start_line.insert(block_name.to_string(), line_num);
        }

        block_stack.push(OnChangeBlock {
            file,
            name: block_name.map(|s| s.to_string()),
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
                r#"found ThenChange "{}:{}" with no matching OnChange"#,
                path.display(),
                line_num,
            ));
        };
        block.end_line = line_num as u32;
        block.then_change =
            Self::build_then_change(files_to_parse, line_num, &path, root_path, &parsed)?;
        Ok(block)
    }

    fn try_find_on_change_captures(data: &[u8]) -> Option<impl Iterator<Item = Captures>> {
        if !ON_CHANGE_PAT.is_match(data) {
            None
        } else {
            Some(ON_CHANGE_PAT.captures_iter(data))
        }
    }

    fn build_byte_pos_to_line_mapping(data: &[u8]) -> Vec<(usize, usize)> {
        let mut v = Vec::new();
        let mut pos = 0;
        let mut line_num = 1;
        for l in data.lines_with_terminator() {
            v.push((pos, line_num));
            line_num += 1;
            pos += l.len();
        }
        v
    }

    fn byte_to_line(mapping: &[(usize, usize)], byte_pos: usize) -> usize {
        let res = mapping.binary_search_by_key(&byte_pos, |(pos, _)| *pos);
        let idx = match res {
            Ok(idx) => idx,
            Err(idx) => idx - 1,
        };
        mapping[idx].1
    }

    pub fn parse<P: AsRef<Path>>(
        path: PathBuf,
        root_path: Option<P>,
    ) -> Result<(Self, HashSet<PathBuf>)> {
        let path_ref = Arc::new(path.clone());
        let root_path = root_path.map(|p| p.as_ref().canonicalize().unwrap());

        // Set of files that need to be parsed based on OnChange targets seen in this file.
        let mut files_to_parse: HashSet<PathBuf> = HashSet::new();

        let mut f = std::fs::File::open(path.as_path())?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;

        // Build a mapping from byte position to line number.
        let byte_pos_to_line_mapping = Self::build_byte_pos_to_line_mapping(&buf);

        let mut blocks: Vec<OnChangeBlock> = Vec::new();
        let mut block_stack: Vec<OnChangeBlock> = Vec::new();
        let mut block_name_to_start_line: HashMap<String, usize> = HashMap::new();

        // Build set of line matches based on byte position in the file.
        let mut matches: Vec<LineMatch> = Vec::new();
        if let Some(captures) = Self::try_find_on_change_captures(&buf) {
            for c in captures {
                // Use start of the overall match as the byte position.
                let pos = c.get(0).unwrap().start();
                if let Some(m) = c.name(ON_CHANGE_GROUP) {
                    matches.push(LineMatch::OnChange(pos, m.as_bytes()));
                } else if let Some(m) = c.name(THEN_CHANGE_GROUP) {
                    matches.push(LineMatch::ThenChange(pos, m.as_bytes()));
                }
            }
        }

        // Sort matches by byte position. This ensures that we get a correct order for OnChange
        // and ThenChange blocks in the file.
        matches.sort_by(|m1, m2| m1.pos().cmp(&m2.pos()));

        for m in matches {
            let line_num = Self::byte_to_line(&byte_pos_to_line_mapping, m.pos());
            let parsed = std::str::from_utf8(m.data()).unwrap();
            match m {
                LineMatch::OnChange(..) => {
                    Self::handle_on_change(
                        path_ref.clone(),
                        parsed,
                        line_num,
                        &mut block_name_to_start_line,
                        &mut block_stack,
                    )?;
                }
                LineMatch::ThenChange(..) => {
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
        }

        if block_stack.len() > 0 {
            // We've hit EOF with an unclosed OnChange block.
            let block = block_stack.last().unwrap();
            return Err(anyhow::anyhow!(
                "reached end of file {} while looking for ThenChange for block \"{}\" which started on line {}",
                path.display(),
                block.name(),
                block.start_line,
            ));
        }

        Ok((File { path, blocks }, files_to_parse))
    }
}
