use criterion::{BenchmarkId, criterion_group, criterion_main, Criterion};

use onchg::test_helpers::*;
use onchg::Parser;

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

    let d = TestDir::new();
    let mut f = RandomOnChangeTree::new(d.path().to_owned(), SEED, 5, 100, 100);
    let (num_directories, num_files) = (100, 1000);
    f.init(num_directories, num_files);

    c.bench_with_input(BenchmarkId::new("directory", num_files), &d, |b, d| {
        b.iter(|| {
            Parser::from_directory(d.path(), true).unwrap();
        });
    });
}

criterion_group!(benches, directory);
criterion_main!(benches);
