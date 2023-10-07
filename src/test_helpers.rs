#![doc(hidden)]

use std::io::Write;
use std::path::Path;

use tempfile::TempDir;

#[derive(Debug)]
pub struct TestDir {
    d: TempDir,
}

impl TestDir {
    pub fn new() -> Self {
        let d = tempfile::tempdir().unwrap();
        Self { d }
    }

    pub fn from_files<P: AsRef<Path>>(files: &[(P, &str)]) -> Self {
        let t = Self::new();
        for (path, content) in files {
            t.write_file(path, content);
        }
        t
    }

    pub fn path(&self) -> &Path {
        self.d.path()
    }

    pub fn write_file<P: AsRef<Path>>(&self, path: P, content: &str) {
        self.write_file_raw(path, content.as_bytes())
    }

    pub fn write_file_raw<P: AsRef<Path>>(&self, path: P, content: &[u8]) {
        let path = self.path().join(path.as_ref());
        if let Some(directory) = path.parent() {
            // Create the directory tree first.
            std::fs::create_dir_all(directory).unwrap();
        }
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content).unwrap();
    }
}

#[derive(Debug)]
pub struct GitRepo(TestDir);

impl GitRepo {
    pub fn new() -> Self {
        let t = TestDir::new();
        std::process::Command::new("git")
            .current_dir(&t.path())
            .arg("init")
            .output()
            .unwrap();
        Self(t)
    }

    // Commits the files to the repo.
    pub fn from_files<P: AsRef<Path>>(files: &[(P, &str)]) -> Self {
        let t = Self::new();
        let mut paths = Vec::new();
        for (path, content) in files {
            t.write_file(path, content);
            paths.push(path);
        }
        t.add_files(Some(&paths));
        t.commit(Some("first commit"));
        t
    }

    pub fn path(&self) -> &Path {
        &self.0.path()
    }

    pub fn write_file<P: AsRef<Path>>(&self, path: P, content: &str) {
        self.0.write_file(path, content)
    }

    #[allow(unused)]
    pub fn write_file_raw<P: AsRef<Path>>(&self, path: P, content: &[u8]) {
        self.0.write_file_raw(path, content)
    }

    pub fn write_and_add_files<P: AsRef<Path>>(&self, files: &[(P, &str)]) {
        for (path, content) in files {
            self.write_file(path, content);
        }
        self.add_files::<&str>(None)
    }

    pub fn add_files<P: AsRef<Path>>(&self, paths: Option<&[P]>) {
        let paths = paths.map(|paths| paths.iter().map(|p| p.as_ref().to_str().unwrap()));

        let mut cmd = std::process::Command::new("git");
        cmd.current_dir(self.path()).arg("add");

        if let Some(paths) = paths {
            cmd.args(paths);
        } else {
            cmd.arg(".");
        }

        let output = cmd.output().unwrap();
        assert!(output.status.success());
    }

    pub fn add_all_files(&self) {
        self.add_files::<&str>(None);
    }

    pub fn commit(&self, msg: Option<&str>) {
        let output = std::process::Command::new("git")
            .current_dir(self.path())
            .arg("commit")
            .arg("-m")
            .arg(msg.unwrap_or("test commit"))
            .output()
            .unwrap();
        assert!(output.status.success());
    }

    #[allow(unused)]
    pub fn diff(&self) -> String {
        let output = std::process::Command::new("git")
            .current_dir(self.path())
            .args(&["diff", "--cached"])
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap()
    }
}
