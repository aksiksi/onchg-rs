// Nice reference: https://rust-cli.github.io/book/tutorial/testing.html
use std::process::Command;

use assert_cmd::prelude::*;

mod helpers;

use helpers::*;
use onchg::test_helpers::*;

#[test]
fn test_directory() {
    let d = TestDir::new();

    let s = std::time::Instant::now();

    // Setup some fake directories and files.
    let mut f = RandomOnChangeTree::new(d.path().to_owned(), 123, 10, 100, 100);
    f.init(10, 100);

    eprintln!("Initialized random directory tree in {:?}", s.elapsed());

    let s = std::time::Instant::now();

    Command::cargo_bin("onchg")
        .unwrap()
        .args(&["directory", "."])
        .current_dir(d.path())
        .assert()
        .success();

    eprintln!("Parsed tree in {:?}", s.elapsed())
}
