use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};

use onchg::test_helpers::*;
use onchg::{Parser, ON_CHANGE_PAT_STR};

const SEED: u64 = 456;

pub fn directory(c: &mut Criterion) {
    let d = TestDir::new();
    let mut f = RandomOnChangeTree::new(d.path().to_owned(), SEED, 5, 100, 100);
    let (num_directories, num_files) = (20, 150);
    f.init(num_directories, num_files);

    c.bench_with_input(BenchmarkId::new("directory", num_files), &d, |b, d| {
        b.iter(|| {
            Parser::from_directory(d.path(), true).unwrap();
        });
    });
    c.bench_with_input(BenchmarkId::new("grep", num_files), &d, |b, d| {
        b.iter(|| {
            let mut cmd = std::process::Command::new("grep");
            cmd.current_dir(d.path())
                .args(&["-rP", ON_CHANGE_PAT_STR, "."])
                .stdout(std::process::Stdio::null());
            assert!(cmd.spawn().unwrap().wait().unwrap().success());
        });
    });

    drop(d);

    let d = TestDir::new();
    let mut f = RandomOnChangeTree::new(d.path().to_owned(), SEED, 5, 100, 100);
    let (num_directories, num_files) = (100, 1000);
    f.init(num_directories, num_files);

    c.bench_with_input(BenchmarkId::new("directory", num_files), &d, |b, d| {
        b.iter(|| {
            Parser::from_directory(d.path(), true).unwrap();
        });
    });
    c.bench_with_input(BenchmarkId::new("grep", num_files), &d, |b, d| {
        b.iter(|| {
            let mut cmd = std::process::Command::new("grep");
            cmd.current_dir(d.path())
                .args(&["-rP", ON_CHANGE_PAT_STR, "."])
                .stdout(std::process::Stdio::null());
            assert!(cmd.spawn().unwrap().wait().unwrap().success());
        });
    });
}

criterion_group!(benches, directory);
criterion_main!(benches);
