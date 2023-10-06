all: test fmt

check:
	cargo check

build: check
	cargo build

test: check
	cargo test -- --nocapture
	cargo test --features git -- --nocapture

ci:
	cargo test --verbose -- --nocapture
	cargo test --verbose --features git -- --nocapture

coverage:
	cargo install cargo-tarpaulin
	cargo tarpaulin

fmt:
	cargo fmt

.PHONY: all build check ci fmt test
