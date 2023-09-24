use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::Result;

use super::{Hunk, Repo};

pub struct GitCli {

}

impl GitCli {
    pub fn new() -> Self {
        Self {}
    }
}

impl Repo for GitCli {
    fn get_staged_files(&self) -> Result<(Vec<PathBuf>, PathBuf)> {
        todo!()
    }

    fn get_staged_hunks(&self) -> Result<BTreeMap<PathBuf, Vec<Hunk>>> {
        todo!()
    }
}
