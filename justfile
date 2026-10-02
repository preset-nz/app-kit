# Standard verbs: install, check. The crate has no runtime of its own and the npm package
# ships unbuilt TypeScript, so `run` and `build` are absent.

default:
    @just --list

[group('setup')]
install:
    cargo fetch
    pnpm install

# Rust: format, lint, test.
[group('quality')]
check-rust:
    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo test

# TypeScript: typecheck and lint.
[group('quality')]
check-ts:
    ./node_modules/.bin/tsc --noEmit
    ./node_modules/.bin/eslint .

[group('quality')]
check: check-rust check-ts

# Apply rustfmt.
[group('quality')]
fmt:
    cargo fmt
