use std::collections::HashSet;
use std::path::Path;

use anyhow::Result;

use crate::core::{FileSet, OnChangeBlock};
use crate::git::{Hunk, Repo};

#[derive(Debug)]
pub struct Parser {
    file_set: FileSet,
}

impl Parser {
    /// Returns changed blocks in the file that are targetable.
    fn find_changed_blocks<'a>(
        hunks: &[Hunk],
        blocks: &[&'a OnChangeBlock],
    ) -> Vec<&'a OnChangeBlock> {
        let mut changed_blocks = HashSet::new();

        // TODO(aksiksi): We can make this faster using a reverse index.
        let mut maybe_overlapping = Vec::new();
        for hunk in hunks {
            for (i, block) in blocks.iter().enumerate() {
                if hunk.is_block_overlap(block.start_line(), block.end_line()) {
                    maybe_overlapping.push((hunk, i));
                }
            }
        }

        for (hunk, block_idx) in maybe_overlapping {
            if changed_blocks.contains(&block_idx) {
                continue;
            }
            let block = &blocks[block_idx];
            if block.is_changed_by_hunk(hunk) {
                changed_blocks.insert(block_idx);
            }
        }

        changed_blocks.into_iter().map(|idx| blocks[idx]).collect()
    }

    pub fn from_git_repo<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        #[cfg(feature = "git")]
        let ((staged_files, repo_path), staged_hunks) = {
            let repo = git2::Repository::discover(path)?;
            (repo.get_staged_files(None)?, repo.get_staged_hunks(None)?)
        };
        #[cfg(not(feature = "git"))]
        let ((staged_files, repo_path), staged_hunks) = {
            (
                crate::git::cli::Cli.get_staged_files(Some(path))?,
                crate::git::cli::Cli.get_staged_hunks(Some(path))?,
            )
        };

        let file_set = FileSet::from_files(staged_files.iter(), &repo_path)?;
        let files_changed: HashSet<&Path> =
            HashSet::from_iter(staged_files.iter().map(|p| p.as_path()));
        let mut blocks_changed: Vec<(&Path, &OnChangeBlock)> = Vec::new();
        let mut targetable_blocks_changed: HashSet<(&Path, &str)> = HashSet::new();

        for (path, hunks) in &staged_hunks {
            let blocks_in_file: Vec<&OnChangeBlock> =
                if let Some(blocks) = file_set.on_change_blocks_in_file(path) {
                    blocks.collect()
                } else {
                    continue;
                };
            let changed_blocks = Self::find_changed_blocks(hunks, &blocks_in_file);
            for block in changed_blocks {
                blocks_changed.push((&path, block));
                targetable_blocks_changed.insert((&path, block.name()));
            }
        }

        // TODO(aksiksi): Collect all on change violations and return them as a list instead
        // of just returning the first one.

        // For each block in the set, check the OnChange target(s) and ensure that they have also changed.
        for (path, block) in blocks_changed {
            let blocks_to_check = block.get_then_change_targets_as_keys(path);
            for (on_change_file, on_change_block) in blocks_to_check {
                if let Some(on_change_block) = on_change_block {
                    if !targetable_blocks_changed.contains(&(on_change_file, on_change_block)) {
                        return Err(anyhow::anyhow!(
                            r#"block "{}" in staged file "{}" has changed, but its OnChange target block "{}:{}" has not"#,
                            block.name(),
                            path.display(),
                            on_change_file.display(),
                            on_change_block,
                        ));
                    }
                } else {
                    if !files_changed.contains(on_change_file) {
                        return Err(anyhow::anyhow!(
                            r#"block "{}" in staged file "{}" has changed, but its OnChange target file "{}" has not"#,
                            block.name(),
                            path.display(),
                            on_change_file.display(),
                        ));
                    }
                }
            }
        }

        Ok(Self { file_set })
    }

    /// Recursively walks through all files in the given path and parses them.
    ///
    /// Note that this method respects .gitignore and .ignore files (via [[ignore]]).
    pub fn from_directory<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file_set = FileSet::from_directory(path)?;
        Ok(Self { file_set })
    }

    pub fn files(&self) -> Vec<&Path> {
        self.file_set.files()
    }
}

#[cfg(test)]
mod test {
    use crate::test_helpers::GitRepo;

    use super::*;

    #[test]
    fn test_from_git_repo() {
        let files = &[
            (
                "f1.txt",
                "OnChange(default)\nabdbbda\nadadd\nThenChange(f2.txt:default)\n",
            ),
            ("f2.txt", "OnChange(default)\nThenChange(f1.txt:default)\n"),
            ("f3.txt", "OnChange(this)\nThenChange(f1.txt)\n"),
        ];
        let d = GitRepo::from_files(files);

        // Delete one line from f1.txt and stage it.
        d.write_file(
            "f1.txt",
            "OnChange(default)\nadadd\nThenChange(f2.txt:default)\n",
        );
        d.add_all_files();
        // This should fail because f1.txt has changed but f2.txt has not.
        assert!(Parser::from_git_repo(d.path()).is_err());

        // Now stage the other file and ensure the parser succeeds.
        d.write_file(
            "f2.txt",
            "OnChange(default)\nadadd\nThenChange(f1.txt:default)\n",
        );
        d.add_all_files();
        Parser::from_git_repo(d.path()).unwrap();

        // Now stage f3 and ensure the parser succeeds.
        d.write_file("f3.txt", "OnChange(this)\nabcde\nThenChange(f1.txt)\n");
        d.add_all_files();
        Parser::from_git_repo(d.path()).unwrap();
    }

    #[test]
    fn test_from_git_repo_multiple_blocks_in_file() {
        let files = &[
            (
                "f1.txt",
                "OnChange(default)\nabdbbda\nadadd\nThenChange(f2.txt:default)\n
                 some\ntext\t\there\n
                 OnChange()\nabdbbda\nadadd\nThenChange(f2.txt:other)\n",
            ),
            (
                "f2.txt",
                "OnChange(default)\nThenChange(f1.txt:default)\n
                 OnChange(other)\nThenChange(f1.txt:default)\n",
            ),
        ];
        let d = GitRepo::from_files(files);

        // Delete one unrelated line from f1.txt and stage it.
        d.write_file(
            "f1.txt",
            "OnChange(default)\nabdbbda\nadadd\nThenChange(f2.txt:default)\n
                 OnChange()\nabdbbda\nadadd\nThenChange(f2.txt:other)\n",
        );
        d.add_all_files();
        // This should pass because no blocks in f1.txt have changed.
        Parser::from_git_repo(d.path()).unwrap();

        // Delete one line from the two blocks in f1.txt and stage it.
        d.write_file(
            "f1.txt",
            "OnChange(default)\nadadd\nThenChange(f2.txt:default)\n
                 OnChange()\nabdbbda\nThenChange(f2.txt:other)\n",
        );
        d.add_all_files();
        // This should fail because f1.txt has changed but f2.txt has not.
        assert!(Parser::from_git_repo(d.path()).is_err());

        // Now change the first block in the other file. The first block in f1 will
        // pass, but second will not.
        d.write_file(
            "f2.txt",
            "OnChange(default)\nabba\nThenChange(f1.txt:default)
                 OnChange(other)\nThenChange(f1.txt:default)\n",
        );
        d.add_all_files();
        assert!(Parser::from_git_repo(d.path()).is_err());

        // Now change the other block in f2 and ensure the parser succeeds.
        d.write_file(
            "f2.txt",
            "OnChange(default)\nabba\nThenChange(f1.txt:default)
                 OnChange(other)\nabbba\nThenChange(f1.txt:default)\n",
        );
        d.add_all_files();
        Parser::from_git_repo(d.path()).unwrap();
    }

    #[test]
    fn test_from_git_repo_multiple_targets() {
        let files = &[
            (
                "f1.txt",
                "OnChange(default)\nabdbbda\nadadd\nThenChange(f2.txt:potato)\n",
            ),
            ("f2.txt", "OnChange(potato)\nThenChange(f1.txt:default)\n"),
            (
                "f3.txt",
                "OnChange()\nThenChange(f1.txt:default, f2.txt:potato, f4.txt:something)\n",
            ),
            (
                "f4.txt",
                "OnChange(something)\nThenChange(f1.txt:default, f2.txt:potato)\n",
            ),
        ];
        let d = GitRepo::from_files(files);

        // Add a line to f3 and f4 and stage them.
        d.write_file(
            "f3.txt",
            "OnChange()\nhello,there!\nThenChange(f1.txt:default, f2.txt:potato, f4.txt:something)\n",
        );
        d.add_all_files();
        // This should fail because f3.txt has changed but f1, f2, and f4 have not.
        assert!(Parser::from_git_repo(d.path()).is_err());

        // Now stage the other files and ensure the parser succeeds.
        d.write_file(
            "f1.txt",
            "OnChange(default)\nadadd\nThenChange(f2.txt:potato)\n",
        );
        d.write_file(
            "f2.txt",
            "OnChange(potato)\nadadd\nThenChange(f1.txt:default)\n",
        );
        d.write_file(
            "f4.txt",
            "OnChange(something)\nnewlinehere\n\nThenChange(f1.txt:default, f2.txt:potato)\n",
        );
        d.add_all_files();
        Parser::from_git_repo(d.path()).unwrap();
    }
}
