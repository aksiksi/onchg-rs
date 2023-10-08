// Nice reference: https://rust-cli.github.io/book/tutorial/testing.html
use std::process::Command;

use assert_cmd::prelude::*;
use predicates::prelude::*;

use onchg::test_helpers::*;

#[test]
fn test_directory() {
    let d = TestDir::new();

    let s = std::time::Instant::now();

    // Setup some fake directories and files.
    let mut f = RandomOnChangeTree::new(d.path().to_owned(), 123, 5, 100, 100);
    f.init(20, 150);

    eprintln!(
        "Initialized random directory tree in {:?} at {}",
        s.elapsed(),
        d.path().display()
    );

    let s = std::time::Instant::now();

    Command::cargo_bin(env!("CARGO_PKG_NAME"))
        .unwrap()
        .args(&["directory", "."])
        .current_dir(d.path())
        .assert()
        .success();

    eprintln!("Parsed tree in {:?}", s.elapsed())
}

#[test]
fn test_git_repo() {
    let d = GitRepo::new();

    let s = std::time::Instant::now();

    // Setup some fake directories and files.
    let mut f = RandomOnChangeTree::new(d.path().to_owned(), 123, 5, 100, 100);
    f.init(20, 150);

    d.add_all_files();
    d.commit(None);

    eprintln!(
        "Initialized random directory tree and repo in {:?} at {}",
        s.elapsed(),
        d.path().display()
    );

    // Touch a few random blocks and stage them.
    for _ in 0..5 {
        f.touch_random_block();
    }
    d.add_all_files();

    let s = std::time::Instant::now();

    Command::cargo_bin(env!("CARGO_PKG_NAME"))
        .unwrap()
        .args(&["repo", "."])
        .current_dir(d.path())
        .assert()
        .failure()
        .stdout(predicate::str::contains("but its OnChange target file"));

    eprintln!("Parsed & validated staged files in {:?}", s.elapsed())
}
