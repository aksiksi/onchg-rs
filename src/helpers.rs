use std::io::Write;
use std::path::Path;

use anyhow::Result;
use tempfile::TempDir;

#[derive(Debug)]
pub struct TestDir {
    d: TempDir,
}

impl TestDir {
    pub fn new() -> Result<Self> {
        let d = tempfile::tempdir()?;
        Ok(Self { d })
    }

    pub fn from_files<P: AsRef<Path>>(files: &[(P, &str)]) -> Result<Self> {
        let t = Self::new()?;
        for (path, content) in files {
            t.write_file(path, content)?;
        }
        Ok(t)
    }

    pub fn path(&self) -> &Path {
        self.d.path()
    }

    pub fn write_file<P: AsRef<Path>>(&self, path: P, content: &str) -> Result<()> {
        self.write_file_raw(path, content.as_bytes())
    }

    pub fn write_file_raw<P: AsRef<Path>>(&self, path: P, content: &[u8]) -> Result<()> {
        let path = self.path().join(path.as_ref());
        let mut f = std::fs::File::create(&path)?;
        f.write_all(content)?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct GitRepo(TestDir);

impl GitRepo {
    pub fn new() -> Result<Self> {
        let t = TestDir::new()?;
        std::process::Command::new("git")
            .current_dir(&t.path())
            .arg("init")
            .output()?;
        Ok(Self(t))
    }

    // Commits the files to the repo.
    pub fn from_files<P: AsRef<Path>>(files: &[(P, &str)]) -> Result<Self> {
        let t = Self::new()?;
        let mut paths = Vec::new();
        for (path, content) in files {
            t.write_file(path, content)?;
            paths.push(path);
        }
        t.add_files(Some(&paths)).unwrap();
        t.commit(Some("first commit")).unwrap();
        Ok(t)
    }

    pub fn path(&self) -> &Path {
        &self.0.path()
    }

    pub fn write_file<P: AsRef<Path>>(&self, path: P, content: &str) -> Result<()> {
        self.0.write_file(path, content)
    }

    #[allow(unused)]
    pub fn write_file_raw<P: AsRef<Path>>(&self, path: P, content: &[u8]) -> Result<()> {
        self.0.write_file_raw(path, content)
    }

    pub fn add_all_files(&self) -> Result<()> {
        self.add_files::<&str>(None)
    }

    pub fn add_files<P: AsRef<Path>>(&self, paths: Option<&[P]>) -> Result<()> {
        let paths = paths.map(|paths| paths.iter().map(|p| p.as_ref().to_str().unwrap()));

        let mut cmd = std::process::Command::new("git");
        cmd.current_dir(self.path()).arg("add");

        if let Some(paths) = paths {
            cmd.args(paths);
        } else {
            cmd.arg(".");
        }

        cmd.output()?;

        Ok(())
    }

    pub fn commit(&self, msg: Option<&str>) -> Result<()> {
        std::process::Command::new("git")
            .current_dir(self.path())
            .arg("commit")
            .arg("-m")
            .arg(msg.unwrap_or("test commit"))
            .output()?;
        Ok(())
    }

    #[allow(unused)]
    pub fn diff(&self) -> Result<String> {
        let output = std::process::Command::new("git")
            .current_dir(self.path())
            .args(&["diff", "--cached"])
            .output()?;
        Ok(String::from_utf8(output.stdout)?)
    }
}
