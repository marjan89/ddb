.PHONY: install build check test

install:
	cargo install --path . --root /opt/homebrew

build:
	cargo build --release

check:
	cargo check

test:
	cargo test
