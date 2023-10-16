use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::Result;
use rayon::prelude::*;

use crate::file::{File, OnChangeBlock};
use crate::git::{Hunk, Repo};
use crate::{ThenChange, ThenChangeTarget};

#[derive(Debug)]
pub struct Parser {
    /// Absolute path to the root directory where this parser was run.
    root_path: PathBuf,
    /// Set of files with _relative_ paths as the key.
    files: BTreeMap<PathBuf, File>,
    /// Total number of blocks parsed.
    num_blocks: usize,
}

impl Parser {
    fn validate_block_target(
        &self,
        path: &Path,
        block: &OnChangeBlock,
        target: &ThenChangeTarget,
        blocks: &HashMap<(&Path, &str), &OnChangeBlock>,
    ) -> Result<()> {
        match target {
            ThenChangeTarget::File(file) => {
                if !self.files.contains_key(file) {
                    return Err(anyhow::anyhow!(
                        r#"block "{}" at "{}:{}" has non-existent ThenChange target "{}""#,
                        block.name(),
                        path.display(),
                        block.end_line(),
                        file.display(),
                    ));
                }
            }
            ThenChangeTarget::Block {
                block: target_block,
                file,
            } => {
                let file = file.as_deref().unwrap_or(path);
                let block_key = (file, target_block.as_str());
                if !blocks.contains_key(&block_key) {
                    return Err(anyhow::anyhow!(
                        r#"block "{}" at "{}:{}" has non-existent ThenChange target "{}:{}""#,
                        block.name(),
                        path.display(),
                        block.end_line(),
                        file.display(),
                        target_block,
                    ));
                }
            }
        }
        Ok(())
    }

    /// Returns a map of all _targetable_ blocks in the file set.
    fn on_change_blocks(&self) -> HashMap<(&Path, &str), &OnChangeBlock> {
        let mut blocks = HashMap::with_capacity(self.num_blocks);
        for (path, file) in self.files.iter() {
            for block in file.blocks.iter() {
                if !block.is_targetable() {
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
                match block.then_change() {
                    ThenChange::NoTarget => {}
                    ThenChange::Targets(targets) => {
                        for t in targets {
                            Self::validate_block_target(&self, path, block, t, &blocks)?;
                        }
                    }
                    ThenChange::Unset => {
                        return Err(anyhow::anyhow!(
                            r#"block "{}" in file "{}" has an unset OnChange target (line {})"#,
                            block.name(),
                            path.display(),
                            block.end_line(),
                        ));
                    }
                }
            }
        }

        Ok(())
    }

    fn validate_root_path<P: AsRef<Path>>(root_path: P) -> Result<()> {
        let root_path = root_path.as_ref();
        if !root_path.exists() {
            Err(anyhow::anyhow!(
                "root path {} does not exist",
                root_path.display(),
            ))
        } else if !root_path.is_dir() {
            Err(anyhow::anyhow!(
                "root path {} is not a directory",
                root_path.display(),
            ))
        } else {
            Ok(())
        }
    }

    /// Builds a parser from the given set of files, as well as any files they depend
    /// on, recursively.
    ///
    /// TODO(aksiksi): Respect .gitignore and .ignore files via [[ignore]].
    ///
    /// NOTE(aksiksi): Work to parallelize the file parsing and traversal logic.
    pub fn from_files<P: AsRef<Path>, Q: AsRef<Path>>(
        paths: impl Iterator<Item = P>,
        root_path: Q,
    ) -> Result<Self> {
        let root_path = root_path.as_ref().canonicalize()?;
        let mut files = BTreeMap::new();

        Self::validate_root_path(&root_path)?;

        let mut file_stack: Vec<PathBuf> = paths
            .map(|p| {
                let path = p.as_ref();
                path.to_owned()
            })
            .collect();

        // Validate provided paths.
        for path in &file_stack {
            let path = root_path.join(path);
            if !path.exists() {
                return Err(anyhow::anyhow!(
                    "file with path \"{}\" does not exist",
                    path.display(),
                ));
            } else if !path.is_file() {
                return Err(anyhow::anyhow!("path \"{}\" is not a file", path.display(),));
            }
        }

        let s = std::time::Instant::now();

        while let Some(path) = file_stack.pop() {
            if let Some((file, files_to_parse)) = File::parse(path.clone(), &root_path)? {
                files.insert(path, file);
                for file_path in files_to_parse {
                    if !files.contains_key(&file_path) {
                        file_stack.push(file_path);
                    }
                }
            }
        }

        let mut num_blocks = 0;
        for file in files.values() {
            num_blocks += file.blocks.len();
        }

        log::info!(
            "Parsed {} files ({} blocks) in {:?}",
            files.len(),
            num_blocks,
            s.elapsed()
        );

        let s = std::time::Instant::now();

        let parser = Self {
            root_path: root_path.to_owned(),
            files,
            num_blocks,
        };
        parser.validate()?;
        log::info!("Validated {} blocks in {:?}", num_blocks, s.elapsed());
        Ok(parser)
    }

    /// Recursively walks through all files in the given path and parses them.
    ///
    /// If ignore is set, this method will respect .gitignore and .ignore files (via [[ignore]]).
    pub fn from_directory<P: AsRef<Path>>(path: P, ignore: bool) -> Result<Self> {
        let root_path = path.as_ref().canonicalize()?;
        let mut files = BTreeMap::new();

        Self::validate_root_path(&root_path)?;

        let s = std::time::Instant::now();

        // Walk the directory (single-threaded).
        let dir_walker = ignore::WalkBuilder::new(&root_path)
            .ignore(ignore)
            .git_global(ignore)
            .git_ignore(ignore)
            .git_exclude(ignore)
            .parents(ignore)
            .build();
        let paths: Vec<PathBuf> = dir_walker
            .filter_map(|e| {
                let path = e.as_ref().unwrap().path().to_owned();
                if !path.is_file() {
                    None
                } else {
                    Some(path.strip_prefix(&root_path).unwrap().to_owned())
                }
            })
            .collect();

        log::info!("Walked {} file paths in {:?}", paths.len(), s.elapsed());

        let s = std::time::Instant::now();

        // Parse the files (multi-threaded).
        let file_items: Vec<_> = paths
            .par_iter()
            .filter_map(|p| {
                if let Some((f, _)) = File::parse(p.to_owned(), &root_path).unwrap() {
                    Some(f)
                } else {
                    None
                }
            })
            .collect();
        for f in file_items {
            files.insert(f.path.clone(), f);
        }

        let mut num_blocks = 0;
        for (_, f) in &files {
            num_blocks += f.blocks.len();
        }

        log::info!(
            "Parsed {} files ({} blocks) in {:?}",
            paths.len(),
            num_blocks,
            s.elapsed()
        );

        let s = std::time::Instant::now();
        let parser = Self {
            root_path: root_path.to_owned(),
            files,
            num_blocks,
        };
        parser.validate()?;
        log::info!("Validated {} blocks in {:?}", num_blocks, s.elapsed());
        Ok(parser)
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

    pub fn paths(&self) -> impl Iterator<Item = &Path> {
        self.files.keys().map(|p| p.as_path())
    }

    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    pub fn num_blocks(&self) -> usize {
        self.num_blocks
    }
}

#[derive(Debug)]
pub struct OnChangeViolation<'a> {
    root_path: &'a Path,
    block: &'a OnChangeBlock,
    target_file: PathBuf,
    target_block: Option<&'a OnChangeBlock>,
}

impl<'a> ToString for OnChangeViolation<'a> {
    fn to_string(&self) -> String {
        if let Some(target_block) = self.target_block {
            format!(
                r#"block "{}" at {}:{} (due to block "{}" at {}:{})"#,
                target_block.name(),
                self.root_path.join(&self.target_file).display(),
                target_block.start_line(),
                self.block.name(),
                self.root_path.join(&self.block.file()).display(),
                self.block.start_line(),
            )
        } else {
            format!(
                r#"file "{}" (due to block "{}" at {}:{})"#,
                self.root_path.join(&self.target_file).display(),
                self.block.name(),
                self.root_path.join(&self.block.file()).display(),
                self.block.start_line(),
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
        let staged_files = {
            let repo = git2::Repository::open(path)?;
            repo.get_staged_files()?
        };
        #[cfg(not(feature = "git"))]
        let staged_files = {
            let cli = crate::git::cli::Cli { repo_path: path };
            cli.get_staged_files()?
        };
        Self::from_files(staged_files.iter(), path)
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
                            .get_block_in_file(on_change_file, on_change_block)
                            .expect("block should exist");
                        violations.push(OnChangeViolation {
                            root_path: &self.root_path,
                            block,
                            target_file: on_change_file.to_owned(),
                            target_block: Some(target_block),
                        });
                    }
                } else if !files_changed.contains(on_change_file) {
                    violations.push(OnChangeViolation {
                        root_path: &self.root_path,
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

        if self.files.len() == 0 {
            return Ok(Vec::new());
        }

        let s = std::time::Instant::now();

        #[cfg(feature = "git")]
        let (staged_files, staged_hunks) = {
            let repo = git2::Repository::open(path)?;
            (repo.get_staged_files()?, repo.get_staged_hunks()?)
        };
        #[cfg(not(feature = "git"))]
        let (staged_files, staged_hunks) = {
            let cli = crate::git::cli::Cli { repo_path: path };
            (cli.get_staged_files()?, cli.get_staged_hunks()?)
        };

        log::info!("Got staged files and hunks in {:?}", s.elapsed());

        let s = std::time::Instant::now();

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

        log::info!("Found changed blocks in {:?}", s.elapsed());

        let s = std::time::Instant::now();

        let violations = self.validate_changed_files_and_blocks(
            files_changed,
            blocks_changed,
            targetable_blocks_changed,
        );

        log::info!("Validated changed files and blocks in {:?}", s.elapsed());

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
        Parser::from_directory(d.path(), false).unwrap();
    }

    #[test]
    fn test_from_files() {
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
        Parser::from_files(file_names, d.path()).unwrap();
    }

    #[test]
    fn test_from_files_file_target() {
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
        Parser::from_files(file_names, d.path()).unwrap();
    }

    #[test]
    fn test_from_files_file_target_parsed() {
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
        // Only provide f3.txt.
        Parser::from_files(std::iter::once("f3.txt"), d.path()).unwrap();
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
        Parser::from_files(file_names, d.path()).unwrap();
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
        let res = Parser::from_files(file_names, d.path());
        assert!(res.is_err());
        let err = res.unwrap_err().to_string();
        assert_eq!(
            err,
            r#"ThenChange target file "f3.txt" at f1.txt:6 does not exist"#
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
        let res = Parser::from_files(file_names, d.path());
        assert!(res.is_err());
        let err = res.unwrap_err().to_string();
        assert_eq!(
            err,
            r#"block "default" at "f1.txt:6" has non-existent ThenChange target "f2.txt:invalid""#,
        );
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
        let res = Parser::from_files(file_names, d.path());
        assert!(res.is_err());
        let err = res.unwrap_err().to_string();
        assert_eq!(
            err,
            r#"duplicate block name "default" found on f1.txt:1 and f1.txt:7"#,
        );
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
        Parser::from_files(file_names, d.path()).unwrap();
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
        let res = Parser::from_files(file_names, d.path());
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
        let res = Parser::from_files(file_names, d.path());
        assert!(res.is_err());
        let err = res.unwrap_err().to_string();
        assert!(err.contains("reached end of file"));
    }

    #[test]
    fn test_from_directory_with_code() {
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
                            # LINT.ThenChange(../f1.html)

                    if __name__ == "__main__":
                        print(A())
                "#},
            ),
        ];
        let d = TestDir::from_files(files);
        Parser::from_directory(d.path(), true).unwrap();
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

    #[test]
    fn test_from_git_repo_single_line_block() {
        let files = &[
            (
                "f1.txt",
                "LINT.OnChange(default)\n aaa LINT.ThenChange(f2.txt:first)\n",
            ),
            (
                "f2.txt",
                indoc! {"
                    LINT.OnChange(first)\n
                    LINT.ThenChange(f1.txt:default)\n
                "},
            ),
        ];
        let d = GitRepo::from_files(files);

        // Delete one character in f1:default and stage it.
        d.write_and_add_files(&[(
            "f1.txt",
            "LINT.OnChange(default)\n aa LINT.ThenChange(f2.txt:first)\n",
        )]);
        parse_and_validate(d.path(), 1);

        d.write_and_add_files(&[(
            "f2.txt",
            indoc! {"
                LINT.OnChange(first)\n
                aaaaaa
                LINT.ThenChange(f1.txt:default)\n
            "},
        )]);
        parse_and_validate(d.path(), 0);
    }

    #[test]
    fn test_validate_empty_git_repo() {
        let d = GitRepo::new();
        parse_and_validate(d.path(), 0);
    }
}
