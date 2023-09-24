use std::path::Path;

mod core;
mod git;
mod parser;

use parser::Parser;

fn main() {
    let git_repo_path = Path::new("../test_repo/abcde");
    let file_set = dbg!(Parser::from_git_repo(git_repo_path).unwrap());

    // For each file, check each block's position against the staged file hunks.
    // If a block has changed, add it to a set.
    // For each block in the set, check the on_change target and ensure that it has also changed.
    // dbg!(&file_set);
    // dbg!(&file_set.blocks());

    // dbg!(&file_set);
}
