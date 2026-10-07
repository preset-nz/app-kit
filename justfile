# Standard verbs: prep, install, check. The crate has no runtime of its own and the npm package
# ships unbuilt TypeScript, so `run` and `build` are absent.

default:
    @just --list

[group('setup')]
prep:
    @echo "node:              $(node --version 2>/dev/null || echo MISSING)"
    @echo "pnpm:              $(pnpm --version 2>/dev/null || echo MISSING)"
    @echo "cargo:             $(cargo --version 2>/dev/null || echo MISSING)"
    @echo "preset-compliance: $(preset-compliance --version 2>/dev/null || echo 'MISSING (cargo binstall preset-compliance)')"
    @echo "knope:             $(knope --version 2>/dev/null || echo 'MISSING (cargo binstall knope)')"

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
    just licences

# Writes the formatters' fixes (Biome, cargo fmt).
[group('quality')]
fmt:
    ./node_modules/.bin/biome check --write .
    cargo fmt

# Licence check against the committed lock. Reads files only, no network.
# Re-resolve with `preset-compliance licences scan` after changing dependencies.
[group('quality')]
licences:
    preset-compliance licences check

# Version, changelog, commit and tag from the conventional commits since the last
# tag (knope.toml). Push and publish stay by hand: guidance runbooks/publish-npm-package.md.
[group('build')]
release:
    knope release

# What `release` would do, without touching anything.
[group('build')]
release-preview:
    knope release --dry-run
