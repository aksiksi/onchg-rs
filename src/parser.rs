use std::collections::HashSet;
use std::path::Path;

use anyhow::Result;

use crate::core::{BlockTarget, FileSet};
use crate::git::Repo;

#[derive(Debug)]
pub struct Parser {
    file_set: FileSet,
}

impl Parser {
    pub fn from_git_repo<P: AsRef<Path>>(path: P) -> Result<Self> {
        let repo = git2::Repository::discover(path.as_ref())?;
        let (staged_files, repo_path) = repo.get_staged_files()?;
        let file_set = FileSet::from_files(&staged_files, &repo_path)?;

        let staged_hunks = repo.get_staged_hunks()?;
        let mut blocks_changed: HashSet<(&Path, &str)> = HashSet::new();

        for (path, hunks) in &staged_hunks {
            let blocks_in_file = if let Some(blocks) = file_set.blocks_in_file(path) {
                blocks
            } else {
                continue;
            };

            // Check each hunk against each block.
            for hunk in hunks {
                for (_, block) in &blocks_in_file {
                    if hunk.is_line_changed_within(block.start_line, block.end_line) {
                        blocks_changed.insert((path.as_path(), block.name.as_str()));
                    }
                }
            }
        }

        // For each block in the set, check the OnChange target and ensure that it has also changed.
        for (path, block_name) in &blocks_changed {
            let path = *path;
            let block = file_set.get_block(path, block_name).unwrap();

            let (on_change_file, on_change_block) = match &block.on_change {
                BlockTarget::None => continue,
                BlockTarget::Block {
                    block: ref target_block,
                    file: target_file,
                } => match target_file {
                    Some(target_file) => (target_file.as_path(), target_block.as_str()),
                    None => (path, target_block.as_str()),
                },
                BlockTarget::Unset => panic!("BlockTarget::Unset should have been resolved by now"),
            };

            if !blocks_changed.contains(&(on_change_file, on_change_block)) {
                return Err(anyhow::anyhow!(
                    r#"Block "{}" in staged file "{}" has changed, but its OnChange target "{}" in "{}" has not"#,
                    block_name,
                    path.display(),
                    on_change_block,
                    on_change_file.display()
                ));
            }
        }

        Ok(Self { file_set })
    }

    /// Recursively walks through all files in the given path and parses them.
    ///
    /// Note that this method respects .gitignore and .ignore files (via [[ignore]]).
    #[allow(unused)]
    pub fn from_directory<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file_set = FileSet::from_directory(path)?;
        Ok(Self { file_set })
    }

    pub fn files(&self) -> Vec<&Path> {
        self.file_set.files()
    }
}
