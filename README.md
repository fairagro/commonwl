# `CommonWL`
[![🦆 Continuous Integration](https://github.com/fairagro/commonwl/actions/workflows/ci.yaml/badge.svg)](https://github.com/fairagro/commonwl/actions/workflows/ci.yaml) ![Crates.io License](https://img.shields.io/crates/l/commonwl) 
![Crates.io Version](https://img.shields.io/crates/v/commonwl) ![Crates.io MSRV](https://img.shields.io/crates/msrv/commonwl) ![Crates.io Total Downloads](https://img.shields.io/crates/d/commonwl)


A Rust-framework for the Common Workflow Language (CWL) that supports parsing and execution.

## Overview
`commonwl` is a Rust-based library crate supporting parsing and executing CWL Workflows and Tools. It is based on the headless task execution framework [crankshaft](https://github.com/stjude-rust-labs/crankshaft). It is developed to being used in the Scientific Workflow Infrastructure ([SciWIn](https://github.com/fairagro/sciwin)).
There is a `conformance` CLI which is being used to evaluate CWL Conformance.

## Getting Started
### Installation
To use `commonwl`, you need to install and setup a Rust environment.
Once Rust is installed, you can add the latest version of `commonwl` be using the following command
```bash
cargo add commonwl 
```
To use the execution engine, the `engine` feature needs to be enabled!
```toml
commonwl = { version = "0.8", features = ["engine"] }
```

## CWL Engine
The CWL Engine features high conformance to the specification, passing all Tests for `Workflow` and `ExpressionTool` and nearly all tests for `CommandLineTool`. The conformance is dependent on the used `TaskBackend`. Currently there are the following existing and planned backends:
| Backend | Status | Overall Conformance |
|---------|--------|---------------------|
| Local   |   ✔️  | ![]( https://img.shields.io/badge/all-99%25-yellow ) ![]( https://img.shields.io/badge/required-97%25-red ) ![]( https://img.shields.io/badge/command_line_tool-98%25-yellow ) ![]( https://img.shields.io/badge/expression_tool-100%25-green )![]( https://img.shields.io/badge/workflow-100%25-green)   |
| Docker* |   ✔️  | ![]( https://img.shields.io/badge/all-99%25-yellow ) ![]( https://img.shields.io/badge/required-97%25-red ) ![]( https://img.shields.io/badge/command_line_tool-98%25-yellow ) ![]( https://img.shields.io/badge/expression_tool-100%25-green )![]( https://img.shields.io/badge/workflow-100%25-green )|
| TES     |   ✔️  | ![]( https://img.shields.io/badge/all-99%25-yellow ) ![]( https://img.shields.io/badge/required-97%25-red ) ![]( https://img.shields.io/badge/command_line_tool-98%25-yellow ) ![]( https://img.shields.io/badge/expression_tool-100%25-green )![]( https://img.shields.io/badge/workflow-100%25-green )   |
| Slurm   |   🧾  | -   |

✔️: Fully operational - 🏗️: Under Construction - 🧾: Planned

*=Uses Docker even if `DockerRequirement` is not specified.

Two tests fail due to f64 overflow outputting 1e42 instead of one with 42 zeros. `serde_json` stores all numbers as f64.

## Storage Backends
File/Directory staging is storage-agnostic and works the same across all task backends:
| Storage | Description |
|---------|--------------|
| Local   | Plain filesystem access. |
| S3      | Any S3-compatible object store (used by the TES backend for remote input/output staging). |
| HTTP(S) | Read-only fetch of `http://`/`https://` input locations. |

## Developing the TES Backend
The TES backend talks to a [GA4GH Task Execution Service](https://github.com/ga4gh/task-execution-schemas). To iterate on it locally you need a TES server plus S3-compatible storage. `.dev/tes_env.sh` spins both up (rustfs + [Funnel](https://github.com/ohsu-comp-bio/funnel)):
```bash
.dev/tes_env.sh start    # start rustfs + funnel, wait until both are healthy
eval "$(.dev/tes_env.sh env)"   # export BACKEND=tes and the S3 credentials/endpoint
cargo build --release -p conformance
BACKEND=tes cwltest --test testdata/cwl/conformance_tests.yaml --tool target/release/conformance
.dev/tes_env.sh stop     # tear both down
```
Funnel has an upstream crash under concurrent load; `.dev/tes_env.sh watchdog` (run alongside a test run) restarts it automatically if that happens.

## License
This work is dual-licensed under Apache 2.0 and MIT . You can choose between one of them if you use this work. 

SPDX-License-Identifier: `Apache-2.0 OR MIT`

**Funded by**


![dfg](https://raw.githubusercontent.com/fairagro/sciwin/refs/heads/main/docs/src/assets/dfg.png)

DFG project number 501899475
