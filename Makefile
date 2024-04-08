all: test fmt

check:
	cargo check

bench:
	cargo bench

bench-git:
	cargo bench --features git

build: check
	cargo build

test: check
	cargo test -- --nocapture --test-threads 1
	cargo test --features git -- --nocapture --test-threads 1

ci:
	cargo test --verbose -- --nocapture
	cargo test --verbose --features git -- --nocapture

coverage:
	# https://github.com/taiki-e/cargo-llvm-cov#installation
	cargo llvm-cov clean --workspace
	cargo llvm-cov --no-report
	cargo llvm-cov --no-report --features git
	cargo llvm-cov report --lcov --output-path lcov.info --ignore-filename-regex test_helpers

fmt:
	cargo fmt

flake:
	nix build -L .#packages.x86_64-linux.default

.PHONY: all bench bench-git build check ci coverage flake fmt test

