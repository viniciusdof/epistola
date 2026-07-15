set dotenv-load
set shell := ["mise", "exec", "--", "sh", "-c"]

# List all available commands
[default]
[group('meta')]
list:
    @just --list

# One-time setup: install the git hooks defined in lefthook.yml.
[group('setup')]
hooks-install:
    lefthook install

# Format, lint, test, and dependency/license checks — what the lefthook pre-push hook runs.
[group('quality')]
check: fmt-check lint test deny shear

# Apply rustfmt formatting.
[group('quality')]
fmt:
    cargo fmt --all

# Check formatting without modifying files.
[group('quality')]
fmt-check:
    cargo fmt --all -- --check

# Lint with clippy, denying all warnings.
[group('quality')]
lint:
    cargo clippy --all-targets --all-features -- -D warnings

# Run the test suite.
[group('quality')]
test:
    cargo test --all-features

# Check licenses, advisories, bans, and sources.
[group('quality')]
deny:
    cargo deny check

# Check for unused workspace dependencies.
[group('quality')]
shear:
    cargo shear

# Line/function/region coverage report.
[group('coverage')]
coverage:
    cargo llvm-cov --all-features --workspace

# Same as `coverage`, but fails if line coverage drops below the floor.
[group('coverage')]
coverage-check:
    cargo llvm-cov --all-features --workspace --fail-under-lines 90

# Build all targets.
[group('build')]
build:
    cargo build --all-targets

# Run the CLI, forwarding ARGS.
[group('build')]
run *ARGS:
    cargo run -p epistola-cli -- {{ ARGS }}

# Run the GPUI desktop client.
[group('build')]
run-gui:
    cargo run -p epistola-gui
