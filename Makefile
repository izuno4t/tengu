.PHONY: help build release run test lint fmt fmt-check spell-check check coverage coverage-report coverage-view doc clean

.DEFAULT_GOAL := help

COVERAGE_REPORT ?= target/coverage/lcov.info
COVERAGE_REPORT_IGNORE_REGEX ?= (/rustc-.*/library/|src/(cli\.rs|tui/.*|llm/(anthropic|google|openai|ollama)\.rs|mcp/(http|stdio)\.rs))
COVERAGE_REPORT_MIN_LINES ?= 0
COVERAGE_VIEWER ?= crv

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
	@echo "  make spell-check"
	@echo "                 Run cspell with the project dictionary"
	@echo "  make check      Run fmt-check, lint, spell-check, and tests"
	@echo "  make coverage   Run the coverage helper"
	@echo "  make coverage-report"
	@echo "                 Generate LCOV at $(COVERAGE_REPORT) for crv"
	@echo "  make coverage-view"
	@echo "                 Generate LCOV and open it with coverage-report-viewer-cli"
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

spell-check:
	cspell --config cspell.json .

check: fmt-check lint spell-check test

coverage:
	scripts/coverage.sh

coverage-report:
	COVERAGE_IGNORE_REGEX='$(COVERAGE_REPORT_IGNORE_REGEX)' COVERAGE_MIN_LINES=$(COVERAGE_REPORT_MIN_LINES) scripts/coverage.sh lcov
	@test -f "$(COVERAGE_REPORT)"
	@echo "LCOV report generated: $(COVERAGE_REPORT)"
	@echo "Scope excludes: $(COVERAGE_REPORT_IGNORE_REGEX)"
	@echo "Open with: $(COVERAGE_VIEWER) --format lcov $(COVERAGE_REPORT)"

coverage-view: coverage-report
	$(COVERAGE_VIEWER) --format lcov "$(COVERAGE_REPORT)"

doc:
	cargo doc --no-deps

clean:
	cargo clean
