use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::core::{FileSet, OnChangeBlock};
use crate::git::{Hunk, Repo};

#[derive(Debug)]
pub struct Parser {
    root_path: PathBuf,
    file_set: FileSet,
}

impl Parser {
    /// Builds a parser from the given set of files, as well as any files they depend
    /// on, recursively.
    ///
    /// TODO(aksiksi): Respect .gitignore and .ignore files via [[ignore]].
    pub fn from_files<P: AsRef<Path>>(
        paths: impl Iterator<Item = P>,
        root_path: PathBuf,
    ) -> Result<Self> {
        let file_set = FileSet::from_files(paths, &root_path)?;
        Ok(Self {
            file_set,
            root_path,
        })
    }

    /// Recursively walks through all files in the given path and parses them.
    ///
    /// Note that this method respects .gitignore and .ignore files (via [[ignore]]).
    pub fn from_directory<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file_set = FileSet::from_directory(&path)?;
        Ok(Self {
            file_set,
            root_path: path.as_ref().to_owned(),
        })
    }

    pub fn files(&self) -> Vec<&Path> {
        self.file_set.files()
    }

    /// Returns a iterator over all of the blocks in a specific file.
    fn on_change_blocks_in_file<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Option<impl Iterator<Item = &OnChangeBlock>> {
        self.file_set.on_change_blocks_in_file(path)
    }

    pub fn root_path(&self) -> &Path {
        &self.root_path
    }
}

#[derive(Debug)]
pub struct OnChangeViolation<'a> {
    block: &'a OnChangeBlock,
    target_file: PathBuf,
    target_block: Option<&'a OnChangeBlock>,
}

impl<'a> ToString for OnChangeViolation<'a> {
    fn to_string(&self) -> String {
        if let Some(target_block) = self.target_block {
            format!(
                r#"block "{}" in staged file at "{}:{}" has changed, but its OnChange target block "{}" at "{}:{}" has not"#,
                self.block.name(),
                self.block.file().display(),
                self.block.start_line(),
                target_block.name(),
                self.target_file.display(),
                target_block.start_line(),
            )
        } else {
            format!(
                r#"block "{}" in staged file "{}:{}" has changed, but its OnChange target file "{}" has not"#,
                self.block.name(),
                self.block.file().display(),
                self.block.start_line(),
                self.target_file.display(),
            )
        }
    }
}

impl Parser {
    /// Returns all changed blocks in the file.
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
            let block = blocks[block_idx];
            if block.is_changed_by_hunk(hunk) {
                changed_blocks.insert(block_idx);
            }
        }

        changed_blocks.into_iter().map(|idx| blocks[idx]).collect()
    }

    pub fn from_git_repo<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        #[cfg(feature = "git")]
        let (staged_files, repo_path) = {
            let repo = git2::Repository::discover(path)?;
            repo.get_staged_files(None)?
        };
        #[cfg(not(feature = "git"))]
        let (staged_files, repo_path) = { crate::git::cli::Cli.get_staged_files(Some(path))? };
        Self::from_files(staged_files.iter(), repo_path)
    }

    // For each block in the set, check the OnChange target(s) and ensure that they have also changed.
    // This will happen _recursively_ for all ThenChange targets. If a violation is detected, it will
    // be returned.
    fn validate_changed_files_and_blocks<'a, 'b>(
        &'a self,
        files_changed: HashSet<&Path>,
        blocks_changed: Vec<&'a OnChangeBlock>,
        targetable_blocks_changed: HashSet<(&Path, &'a str)>,
    ) -> Vec<OnChangeViolation<'a>>
    where
        'a: 'b,
    {
        let mut violations = Vec::new();

        // Treat the blocks_changed list as a stack. This allows us to run a DFS on ThenChange targets.
        for block in blocks_changed {
            let blocks_to_check = block.get_then_change_targets_as_keys();
            for (on_change_file, on_change_block) in blocks_to_check {
                if let Some(on_change_block) = on_change_block {
                    if !targetable_blocks_changed.contains(&(on_change_file, on_change_block)) {
                        let target_block = self
                            .file_set
                            .get_block_in_file(on_change_file, on_change_block)
                            .expect("block should exist");
                        violations.push(OnChangeViolation {
                            block,
                            target_file: on_change_file.to_owned(),
                            target_block: Some(target_block),
                        });
                    }
                } else if !files_changed.contains(on_change_file) {
                    violations.push(OnChangeViolation {
                        block,
                        target_file: on_change_file.to_owned(),
                        target_block: None,
                    });
                }
            }
        }

        violations
    }

    pub fn validate_git_repo(&self) -> Result<Vec<OnChangeViolation<'_>>> {
        let path = self.root_path.as_path();

        #[cfg(feature = "git")]
        let ((staged_files, _), staged_hunks) = {
            // We already have a correct Git path.
            let repo = git2::Repository::open(path)?;
            (repo.get_staged_files(None)?, repo.get_staged_hunks(None)?)
        };
        #[cfg(not(feature = "git"))]
        let ((staged_files, _), staged_hunks) = {
            (
                crate::git::cli::Cli.get_staged_files(Some(path))?,
                crate::git::cli::Cli.get_staged_hunks(Some(path))?,
            )
        };

        let files_changed: HashSet<&Path> =
            HashSet::from_iter(staged_files.iter().map(|p| p.as_path()));
        let mut blocks_changed: Vec<&OnChangeBlock> = Vec::new();
        let mut targetable_blocks_changed: HashSet<(&Path, &str)> = HashSet::new();

        for (path, hunks) in &staged_hunks {
            let blocks_in_file: Vec<&OnChangeBlock> =
                if let Some(blocks) = self.on_change_blocks_in_file(path) {
                    blocks.collect()
                } else {
                    continue;
                };
            let changed_blocks = Self::find_changed_blocks(hunks, &blocks_in_file);
            for block in changed_blocks {
                blocks_changed.push(block);
                if block.is_targetable() {
                    targetable_blocks_changed.insert((&path, block.name()));
                }
            }
        }

        let violations = self.validate_changed_files_and_blocks(
            files_changed,
            blocks_changed,
            targetable_blocks_changed,
        );

        Ok(violations)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_helpers::*;
    use indoc::indoc;

    fn parse_and_validate(path: &Path, num_violations: usize) {
        let p = Parser::from_git_repo(path).unwrap();
        assert_eq!(p.validate_git_repo().unwrap().len(), num_violations);
    }

    #[test]
    fn test_from_directory() {
        let files = &[
            (
                "f1.html",
                indoc! {"
                    <html>
                        <body>
                            <!-- LINT.OnChange(html) -->
                            <h1>Hello</h1>
                            <!-- LINT.ThenChange(f2.cpp:cpp) -->
                            <p>abc</p>
                            <!-- LINT.OnChange(other-html) -->
                            <p>def</p>
                            <!-- LINT.ThenChange() -->
                        </body>
                    </html>
                "},
            ),
            (
                "f2.cpp",
                indoc! {r#"
                    class A {
                        A() {
                            // LINT.OnChange()
                            int a = 10;
                            // LINT.ThenChange(abc/f3.py:python)
                        }
                    }
                    int main() {
                        // LINT.OnChange(cpp)
                        printf("Hello, world!\n);
                        // LINT.ThenChange(f1.html:html)
                    }
                "#},
            ),
            (
                "abc/f3.py",
                indoc! {r#"
                    class A:
                        def __init__(self):
                            # LINT.OnChange(python)
                            self.v = 10
                            # LINT.ThenChange(f1.html)

                    if __name__ == "__main__":
                        print(A())
                "#},
            ),
        ];
        let d = TestDir::from_files(files);
        Parser::from_directory(d.path()).unwrap();
    }

    #[test]
    fn test_from_git_repo() {
        let files = &[
            (
                "f1.txt",
                "LINT.OnChange(default)\n
                 abdbbda\nadadd\n
                 LINT.ThenChange(f2.txt:default)\n",
            ),
            (
                "f2.txt",
                "LINT.OnChange(default)\n
                 LINT.ThenChange(f1.txt:default)\n",
            ),
            (
                "f3.txt",
                "LINT.OnChange(this)\n
                 LINT.ThenChange(f1.txt)\n",
            ),
        ];
        let d = GitRepo::from_files(files);

        // Delete one line from f1.txt and stage it.
        d.write_and_add_files(&[(
            "f1.txt",
            "LINT.OnChange(default)\n
             adadd\n
             LINT.ThenChange(f2.txt:default)\n",
        )]);
        // This should fail because f1.txt has changed but f2.txt has not.
        parse_and_validate(d.path(), 1);

        // Now stage the other file and ensure the parser succeeds.
        d.write_and_add_files(&[(
            "f2.txt",
            "LINT.OnChange(default)\n
             adadd\n
             LINT.ThenChange(f1.txt:default)\n",
        )]);
        parse_and_validate(d.path(), 0);

        // Now stage f3 and ensure the parser succeeds.
        d.write_and_add_files(&[(
            "f3.txt",
            "LINT.OnChange(this)\n
             abcde\n
             LINT.ThenChange(f1.txt)\n",
        )]);
        parse_and_validate(d.path(), 0);
    }

    #[test]
    fn test_from_git_repo_relative_path_priority() {
        let files = &[
            // Files at the root.
            (
                "f1.txt",
                "LINT.OnChange(default)\n
                 abdbbda\nadadd\n
                 LINT.ThenChange(f2.txt:default)\n",
            ),
            (
                "f2.txt",
                "LINT.OnChange(default)\n
                 LINT.ThenChange(abc/f1.txt:default)\n",
            ),
            // Files in a subdirectory.
            (
                "abc/f1.txt",
                "LINT.OnChange(default)\n
                 abdbbda\nadadd\n
                 LINT.ThenChange(f2.txt:default)\n",
            ),
            (
                "abc/f2.txt",
                "LINT.OnChange(default)\n
                 LINT.ThenChange(f1.txt:default)\n",
            ),
        ];
        let d = GitRepo::from_files(files);

        // Change and stage both abc/f1.txt and f2.txt.
        // This should fail because abc/f1.txt depends on abc/f2.txt, not f2.txt.
        d.write_and_add_files(&[
            (
                "abc/f1.txt",
                "LINT.OnChange(default)\n
                 adadd\n
                 LINT.ThenChange(f2.txt:default)\n",
            ),
            (
                "f2.txt",
                "LINT.OnChange(default)\n
                 adadd\n
                 LINT.ThenChange(abc/f1.txt:default)\n",
            ),
        ]);
        parse_and_validate(d.path(), 1);

        // Now change and stage abc/f2.txt.
        d.write_and_add_files(&[(
            "abc/f2.txt",
            "LINT.OnChange(default)\n
             abc\n
             LINT.ThenChange(f1.txt:default)\n",
        )]);
        parse_and_validate(d.path(), 0);
    }

    #[test]
    fn test_from_git_repo_multiple_blocks_in_file() {
        let files = &[
            (
                "f1.txt",
                indoc! {"
                    LINT.OnChange(default)\n
                    abdbbda\nadadd\n
                    LINT.ThenChange(f2.txt:default)\n
                    some\ntext\t\there\n
                    LINT.OnChange()\n
                    abdbbda\nadadd\n
                    LINT.ThenChange(f2.txt:other)\n
                "},
            ),
            (
                "f2.txt",
                indoc! {"
                    LINT.OnChange(default)\n
                    LINT.ThenChange(f1.txt:default)\n
                    LINT.OnChange(other)\n
                    LINT.ThenChange(f1.txt:default)\n
                "},
            ),
        ];
        let d = GitRepo::from_files(files);

        // Delete one unrelated line from f1.txt and stage it.
        d.write_and_add_files(&[(
            "f1.txt",
            indoc! {"
                LINT.OnChange(default)\n
                abdbbda\nadadd\n
                LINT.ThenChange(f2.txt:default)\n
                LINT.OnChange()\n
                abdbbda\nadadd\n
                LINT.ThenChange(f2.txt:other)\n
            "},
        )]);
        // This should pass because no blocks in f1.txt have changed.
        parse_and_validate(d.path(), 0);

        // Delete one line from the two blocks in f1.txt and stage it.
        d.write_and_add_files(&[(
            "f1.txt",
            indoc! {"
                LINT.OnChange(default)\n
                abdbbda\n
                LINT.ThenChange(f2.txt:default)\n
                LINT.OnChange()\n
                abdbbda\n
                LINT.ThenChange(f2.txt:other)\n
            "},
        )]);
        // This should fail because f1.txt has changed but f2.txt has not.
        parse_and_validate(d.path(), 2);

        // Now change the first block in the other file. The first block in f1 will
        // pass, but second will not.
        d.write_and_add_files(&[(
            "f2.txt",
            indoc! {"
                LINT.OnChange(default)\n
                abba\n
                LINT.ThenChange(f1.txt:default)\n
                LINT.OnChange(other)\n
                LINT.ThenChange(f1.txt:default)\n
            "},
        )]);
        parse_and_validate(d.path(), 1);

        // Now change the other block in f2 and ensure the parser succeeds.
        d.write_and_add_files(&[(
            "f2.txt",
            indoc! {"
                LINT.OnChange(default)\n
                abba\n
                LINT.ThenChange(f1.txt:default)\n
                LINT.OnChange(other)\n
                abba\n
                LINT.ThenChange(f1.txt:default)\n
            "},
        )]);
        parse_and_validate(d.path(), 0);
    }

    #[test]
    fn test_from_git_repo_multiple_targets() {
        let files = &[
            (
                "f1.txt",
                "LINT.OnChange(default)\n
                 abdbbda\nadadd\n
                 LINT.ThenChange(f2.txt:potato)\n",
            ),
            (
                "f2.txt",
                "LINT.OnChange(potato)\n
                 LINT.ThenChange(f1.txt:default)\n",
            ),
            (
                "f3.txt",
                "LINT.OnChange()\n
                 LINT.ThenChange(f1.txt:default, f2.txt:potato, f4.txt:something)\n",
            ),
            (
                "f4.txt",
                "LINT.OnChange(something)\n
                 LINT.ThenChange(f1.txt:default, f2.txt:potato)\n",
            ),
        ];
        let d = GitRepo::from_files(files);

        // Add a line to f3 and stage it.
        d.write_and_add_files(&[(
            "f3.txt",
            "LINT.OnChange()\n
             hello,there!\n
             LINT.ThenChange(f1.txt:default, f2.txt:potato, f4.txt:something)\n",
        )]);
        parse_and_validate(d.path(), 3);

        // Now stage the other files and ensure the parser succeeds.
        d.write_and_add_files(&[
            (
                "f1.txt",
                "LINT.OnChange(default)\n
                 adadd\n
                 LINT.ThenChange(f2.txt:potato)\n",
            ),
            (
                "f2.txt",
                "LINT.OnChange(potato)\n
                 adadd\n
                 LINT.ThenChange(f1.txt:default)\n",
            ),
            (
                "f4.txt",
                "LINT.OnChange(something)\n
                 newlinehere\n\n
                 LINT.ThenChange(f1.txt:default, f2.txt:potato)\n",
            ),
        ]);
        parse_and_validate(d.path(), 0);
    }

    #[test]
    fn test_from_git_repo_nested_blocks() {
        let files = &[
            (
                "f1.txt",
                "LINT.OnChange(outer)
                \nabdbbda\nadadd
                \n
                LINT.OnChange(inner)\n
                bbbb\n
                LINT.ThenChange(f2.txt:first)\n
                \n
                LINT.ThenChange()\n",
            ),
            (
                "f2.txt",
                "LINT.OnChange(first)\n
                 LINT.ThenChange(f1.txt:inner)\n
                 LINT.OnChange(second)\n
                 LINT.ThenChange()\n",
            ),
        ];
        let d = GitRepo::from_files(files);

        // Delete one line in f1:inner and stage it. Also stage f2:first.
        d.write_and_add_files(&[
            (
                "f1.txt",
                "LINT.OnChange(outer)
                \nabdbbda\nadadd
                \n
                LINT.OnChange(inner)\n
                LINT.ThenChange(f2.txt:first)\n
                \n
                LINT.ThenChange()\n",
            ),
            (
                "f2.txt",
                "LINT.OnChange(first)\naaaa\nLINT.ThenChange(f1.txt:inner)\n
                 LINT.OnChange(second)\nLINT.ThenChange()\n",
            ),
        ]);
        parse_and_validate(d.path(), 0);
    }
}
