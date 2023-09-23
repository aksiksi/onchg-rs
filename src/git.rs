use std::path::{Path, PathBuf};

use anyhow::{Error, Result};
use git2::Repository;

pub fn get_staged_file_paths<P: AsRef<Path>>(path: P, is_repo_path: bool) -> Result<(Vec<PathBuf>, PathBuf, Repository), Error> {
    let repo = if is_repo_path {
        Repository::open(path.as_ref())?
    } else {
        Repository::discover(path.as_ref())?
    };
    let repo_path = repo.path().parent().unwrap();
    let index = repo.index()?;
    let mut paths = Vec::new();
    for entry in index.iter() {
        let s = String::from_utf8(entry.path)?;
        let p = repo_path.join(Path::new(&s));
        paths.push(p);
    }
    Ok((paths, repo_path.to_owned(), repo))
}

#[allow(dead_code)]
#[allow(unused_variables)]
pub fn get_staged_hunks(repo: &Repository) -> Result<()> {
    let tree = repo.head()?.peel_to_tree()?;
    let diff = repo.diff_tree_to_index(Some(&tree), None, None)?;
    diff.foreach(
        &mut |delta, hunk| {
            // println!("delta: {:?}", delta);
            // println!("hunk: {:?}", hunk);
            true
        },
        None,
        None,
        Some(&mut |delta, hunk, line| {
            // println!("delta: {:?}", delta);
            // println!("hunk: {:?}", hunk);
            println!("line: {:?}", line);
            true
        }),
    )?;
    Ok(())
}
