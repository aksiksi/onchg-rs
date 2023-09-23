use std::path::Path;

mod core;
mod git;

use core::FileSet;

fn main() {
    let git_repo_path = Path::new("../test_repo/abcde");
    let file_set = FileSet::parse_staged_files(git_repo_path, false).unwrap();

    // For each file, check each block's position against the staged file hunks.
    // If a block has changed, add it to a set.
    // For each block in the set, check the on_change target and ensure that it has also changed.
    dbg!(&file_set);
    dbg!(&file_set.blocks());

    let path = Path::new("../test_repo/");
    let file_set = FileSet::parse(path).unwrap();
    dbg!(&file_set);
}
