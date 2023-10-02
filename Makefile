all: test fmt

check:
	cargo check

build: check
	cargo build

test: build
	cargo test -- --nocapture
	cargo test --features git -- --nocapture

ci: build
	cargo test --verbose -- --nocapture
	cargo test --verbose --features git -- --nocapture

fmt:
	cargo fmt

.PHONY: all build check ci fmt test
