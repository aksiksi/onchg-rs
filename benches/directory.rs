use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};

use onchg::test_helpers::*;
use onchg::{Parser, ON_CHANGE_PAT_STR};

const SEED: u64 = 456;

pub fn directory_sparse(c: &mut Criterion) {
    // ripgrep supports this special syntax, while the Rust regex crate supports the
    // native syntax.
    let ripgrep_on_change_pat = ON_CHANGE_PAT_STR.replace("?<", "?P<");

    let d = TestDir::new();
    let mut f = RandomOnChangeTree::new(d.path().to_owned(), SEED, 5, 0, 10, 100, 100);
    let (num_directories, num_files) = (20, 150);
    f.init(num_directories, num_files);

    c.bench_with_input(
        BenchmarkId::new("directory-sparse", num_files),
        &d,
        |b, d| {
            b.iter(|| {
                Parser::from_directory(d.path(), true).unwrap();
            });
        },
    );
    c.bench_with_input(BenchmarkId::new("grep-sparse", num_files), &d, |b, d| {
        b.iter(|| {
            let mut cmd = std::process::Command::new("grep");
            cmd.current_dir(d.path())
                .args(&["-rnP", ON_CHANGE_PAT_STR, "."])
                .stdout(std::process::Stdio::null());
            assert!(cmd.spawn().unwrap().wait().unwrap().success());
        });
    });
    c.bench_with_input(BenchmarkId::new("ripgrep-sparse", num_files), &d, |b, d| {
        b.iter(|| {
            let mut cmd = std::process::Command::new("rg");
            cmd.current_dir(d.path())
                .args(&["-n", &ripgrep_on_change_pat, "."])
                .stdout(std::process::Stdio::null());
            assert!(cmd.spawn().unwrap().wait().unwrap().success());
        });
    });

    drop(d);

    let d = TestDir::new();
    let mut f = RandomOnChangeTree::new(d.path().to_owned(), SEED, 5, 0, 10, 100, 100);
    let (num_directories, num_files) = (100, 1000);
    f.init(num_directories, num_files);

    c.bench_with_input(
        BenchmarkId::new("directory-sparse", num_files),
        &d,
        |b, d| {
            b.iter(|| {
                Parser::from_directory(d.path(), true).unwrap();
            });
        },
    );
    c.bench_with_input(BenchmarkId::new("grep-sparse", num_files), &d, |b, d| {
        b.iter(|| {
            let mut cmd = std::process::Command::new("grep");
            cmd.current_dir(d.path())
                .args(&["-rnP", ON_CHANGE_PAT_STR, "."])
                .stdout(std::process::Stdio::null());
            assert!(cmd.spawn().unwrap().wait().unwrap().success());
        });
    });
    c.bench_with_input(BenchmarkId::new("ripgrep-sparse", num_files), &d, |b, d| {
        b.iter(|| {
            let mut cmd = std::process::Command::new("rg");
            cmd.current_dir(d.path())
                .args(&["-n", &ripgrep_on_change_pat, "."])
                .stdout(std::process::Stdio::null());
            assert!(cmd.spawn().unwrap().wait().unwrap().success());
        });
    });
}

pub fn directory_dense(c: &mut Criterion) {
    // ripgrep supports this special syntax, while the Rust regex crate supports the
    // native syntax.
    let ripgrep_on_change_pat = ON_CHANGE_PAT_STR.replace("?<", "?P<");

    let d = TestDir::new();
    let mut f = RandomOnChangeTree::new(d.path().to_owned(), SEED, 5, 50, 100, 100, 100);
    let (num_directories, num_files) = (20, 150);
    f.init(num_directories, num_files);

    c.bench_with_input(
        BenchmarkId::new("directory-dense", num_files),
        &d,
        |b, d| {
            b.iter(|| {
                Parser::from_directory(d.path(), true).unwrap();
            });
        },
    );
    c.bench_with_input(BenchmarkId::new("grep-dense", num_files), &d, |b, d| {
        b.iter(|| {
            let mut cmd = std::process::Command::new("grep");
            cmd.current_dir(d.path())
                .args(&["-rnP", ON_CHANGE_PAT_STR, "."])
                .stdout(std::process::Stdio::null());
            assert!(cmd.spawn().unwrap().wait().unwrap().success());
        });
    });
    c.bench_with_input(BenchmarkId::new("ripgrep-dense", num_files), &d, |b, d| {
        b.iter(|| {
            let mut cmd = std::process::Command::new("rg");
            cmd.current_dir(d.path())
                .args(&["-n", &ripgrep_on_change_pat, "."])
                .stdout(std::process::Stdio::null());
            assert!(cmd.spawn().unwrap().wait().unwrap().success());
        });
    });

    drop(d);

    let d = TestDir::new();
    let mut f = RandomOnChangeTree::new(d.path().to_owned(), SEED, 5, 50, 100, 100, 100);
    let (num_directories, num_files) = (100, 1000);
    f.init(num_directories, num_files);

    c.bench_with_input(
        BenchmarkId::new("directory-dense", num_files),
        &d,
        |b, d| {
            b.iter(|| {
                Parser::from_directory(d.path(), true).unwrap();
            });
        },
    );
    c.bench_with_input(BenchmarkId::new("grep-dense", num_files), &d, |b, d| {
        b.iter(|| {
            let mut cmd = std::process::Command::new("grep");
            cmd.current_dir(d.path())
                .args(&["-rnP", ON_CHANGE_PAT_STR, "."])
                .stdout(std::process::Stdio::null());
            assert!(cmd.spawn().unwrap().wait().unwrap().success());
        });
    });
    c.bench_with_input(BenchmarkId::new("ripgrep-dense", num_files), &d, |b, d| {
        b.iter(|| {
            let mut cmd = std::process::Command::new("rg");
            cmd.current_dir(d.path())
                .args(&["-n", &ripgrep_on_change_pat, "."])
                .stdout(std::process::Stdio::null());
            assert!(cmd.spawn().unwrap().wait().unwrap().success());
        });
    });
}


criterion_group!(benches, directory_sparse, directory_dense);
criterion_main!(benches);
