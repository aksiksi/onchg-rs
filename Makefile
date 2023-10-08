all: test fmt

check:
	cargo check

benches:
	cargo bench

build: check
	cargo build

test: check
	cargo test -- --nocapture
	cargo test --features git -- --nocapture

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

.PHONY: all benches build check ci coverage fmt test
