.PHONY: help build release run test lint fmt fmt-check check coverage doc clean

.DEFAULT_GOAL := help

help:
	@echo "Tengu development commands"
	@echo ""
	@echo "Usage:"
	@echo "  make build      Compile the debug binary"
	@echo "  make release    Compile the release binary"
	@echo "  make run        Run the CLI locally"
	@echo "  make test       Run all tests"
	@echo "  make lint       Run clippy with warnings denied"
	@echo "  make fmt        Format Rust sources"
	@echo "  make fmt-check  Check Rust formatting"
	@echo "  make check      Run fmt-check, lint, and tests"
	@echo "  make coverage   Run the coverage helper"
	@echo "  make doc        Build API documentation"
	@echo "  make clean      Remove Cargo build artifacts"

build:
	cargo build

release:
	cargo build --release

run:
	cargo run

test:
	cargo test

lint:
	cargo clippy -- -D warnings

fmt:
	cargo fmt

fmt-check:
	cargo fmt --check

check: fmt-check lint test

coverage:
	scripts/coverage.sh

doc:
	cargo doc --no-deps

clean:
	cargo clean
