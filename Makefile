all: test fmt

check:
	cargo check

build: check
	cargo build

test: build
	./test.sh

fmt:
	cargo fmt

.PHONY: all build check fmt test
