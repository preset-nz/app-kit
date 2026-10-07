# Standard verbs: install, check. The crate has no runtime of its own and the npm package
# ships unbuilt TypeScript, so `run` and `build` are absent.

default:
    @just --list

[group('setup')]
install:
    cargo fetch
    pnpm install
    ./node_modules/.bin/lefthook install

# Rust: format, lint, test.
[group('quality')]
check-rust:
    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo test

# TypeScript: typecheck, Biome (lint and format check).
[group('quality')]
check-ts:
    ./node_modules/.bin/tsc --noEmit
    ./node_modules/.bin/biome check .

[group('quality')]
check: check-rust check-ts

# Writes the formatters' fixes (Biome, cargo fmt).
[group('quality')]
fmt:
    ./node_modules/.bin/biome check --write .
    cargo fmt
