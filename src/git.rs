use std::path::{Path, PathBuf};

use anyhow::Error;
use git2::Repository;

pub fn get_staged_file_paths<P: AsRef<Path>>(path: P, is_repo_path: bool) -> Result<(Vec<PathBuf>, PathBuf), Error> {
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
    Ok((paths, repo.path().to_owned()))
}

#[allow(dead_code)]
#[allow(unused_variables)]
pub fn get_staged_hunks<P: AsRef<Path>>(path: P, is_repo_path: bool) {
    todo!()
}
