use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};

use onchg::test_helpers::*;
use onchg::Parser;

const SEED: u64 = 456;

pub fn git_repo(c: &mut Criterion) {
    env_logger::builder().is_test(true).init();

    let d = GitRepo::new();

    let s = std::time::Instant::now();

    let mut f = RandomOnChangeTree::new(d.path().to_owned(), SEED, 5, 50, 100, 100, 100);
    f.init(100, 1000);
    d.add_all_files();
    d.commit(None);

    let n = 200;
    for _ in 0..n {
        f.touch_random_block();
    }
    d.add_all_files();

    log::info!(
        "Created random tree {} & touched {} blocks in {:?}",
        d.path().display(),
        n,
        s.elapsed(),
    );

    c.bench_with_input(BenchmarkId::new("git-repo", n), &d, |b, d| {
        b.iter(|| {
            let p = Parser::from_git_repo(d.path()).unwrap();
            let violations = p.validate_git_repo().unwrap();
            assert_eq!(violations.len(), 358);
        });
    });
}

criterion_group!(benches, git_repo);
criterion_main!(benches);
