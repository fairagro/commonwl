# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

`commonwl` is a Rust framework for the Common Workflow Language (CWL): parsing (`CommandLineTool`, `Workflow`, `ExpressionTool`, `Operation`) plus an execution engine. It is developed for use in the Scientific Workflow Infrastructure ([SciWIn](https://github.com/fairagro/sciwin), sibling repo `../sciwin`). Execution is built on the headless task runner [crankshaft](https://github.com/stjude-rust-labs/crankshaft).

This is a Cargo workspace with these crates (`crates/`):
- `commonwl` — the public-facing library crate; re-exports `cwl_core` and, behind the `engine` feature, `cwl_engine`/`cwl_engine_storage` as the `engine`/`storage` modules.
- `core` (`cwl_core`) — CWL document model: parsing, loading, validation, requirements, packed documents.
- `engine` (`cwl_engine`) — execution engine: `backend/` (local, docker, TES), `environment/` (runtime env/workdir setup), `io/` (file/directory/JSON staging), `schema/` (schema validation), plus `command.rs`, `expression.rs` (CWL JS expressions via `boa_engine`), `scatter.rs`, `workflow.rs`.
- `storage` (`cwl_engine_storage`) — storage backends used for I/O staging: local filesystem, S3 (`aws-sdk-s3`), HTTP(S) read-only fetch.
- `salad` / `salad_derive` (`cwl_salad`) — Schema Salad-style deserialization DSL and its derive macro, used to define the CWL document schema.
- `conformance` — CLI that runs the engine against the official CWL conformance test suite (used with `cwltest`).
- `lsp` (`cwl-lsp`) — a language server (diagnostics, symbols, formatting) for CWL documents, built on `tower-lsp-server`. Under construction.

Feature flags on the `commonwl` crate: no default features for parsing-only use; `engine` enables the execution engine; `tes` (with `engine`) enables the GA4GH Task Execution Service (TES) backend.

Task backends and their status: Local and Docker (fully operational, Docker always used even without an explicit `DockerRequirement`), TES (fully operational), Slurm (planned). Storage backends (used the same way across all task backends): Local filesystem, S3-compatible, HTTP(S) read-only.

## Common commands

```bash
cargo build --workspace
cargo clippy --workspace -- -W clippy::pedantic     # matches CI lint step
cargo nextest run --workspace --no-fail-fast        # matches CI test step (requires cargo-nextest)
cargo test -p cwl_core some_test_name               # run a single test
```

This repo uses git submodules (e.g. the CWL v1.2 conformance test suite under `testdata/`) — clone/update with `--recurse-submodules` / `git submodule update --init`. On Windows, submodules are skipped in CI because a file in `cwl_v1.2` has a `:` in its name, which Windows paths don't support.

### TES backend local dev loop

The TES backend talks to a GA4GH Task Execution Service; testing it locally needs a TES server plus S3-compatible storage. `.dev/tes_env.sh` spins up rustfs + [Funnel](https://github.com/ohsu-comp-bio/funnel):

```bash
.dev/tes_env.sh start                    # start rustfs + funnel, wait until healthy
eval "$(.dev/tes_env.sh env)"            # export BACKEND=tes and S3 credentials/endpoint
cargo build --release -p conformance
BACKEND=tes cwltest --test testdata/cwl/conformance_tests.yaml --tool target/release/conformance
# or, targeting the Rust test suite directly:
BACKEND=tes cargo test -p cwl_engine --test conformance test_conformance_tes -- --nocapture
.dev/tes_env.sh stop                     # tear both down
```

Funnel has an upstream crash under concurrent S3 downloads (a Go panic in its debug-log formatter); run `.dev/tes_env.sh watchdog` alongside a test run to auto-restart it so a mid-suite crash only loses the in-flight tests.

Two conformance tests are expected to fail: f64 overflow outputs `1e42` instead of the literal with 42 zeros, because `serde_json` stores all numbers as f64.

## CI

CI (`.github/workflows/ci.yaml`) runs on Linux/macOS/Windows: `cargo clippy --workspace -- -W clippy::pedantic`, then `cargo nextest run --workspace --no-fail-fast`, then coverage via `cargo tarpaulin`. Windows uses podman instead of Docker for the runner.
