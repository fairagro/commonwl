# CommonWL
[![🦆 Continuous Integration](https://github.com/fairagro/commonwl/actions/workflows/ci.yaml/badge.svg)](https://github.com/fairagro/commonwl/actions/workflows/ci.yaml)

A Rust-framework for the Common Workflow Language (CWL) that supports parsing and execution.

## Overview
`commonwl` is a Rust-based library crate supporting parsing and executing CWL Workflows and Tools. It is based on the headless task execution framework [crankshaft](https://github.com/stjude-rust-labs/crankshaft). It is developed to being used in the Scientific Workflow Infrastructure ([SciWIn](https://github.com/fairagro/sciwin)).
There is a `conformance` CLI which is being used to evaluate CWL Conformance.

## Getting Started
### Installation
To use `commonwl`, you need to install and setup a Rust environment.
Once Rust is installed, you can add the latest version of `commonwl` be using the following command
```bash
cargo add commonwl # will not work as not published yet!
```
To use the execution engine, the `engine` feature needs to be enabled!
```toml
commonwl = { version = "0.0.1", features = ["engine"] }
```

## CWL Engine
The CWL Engine features high conformance to the specification, passing all Tests for `Workflow` and `ExpressionTool` and nearly all tests for `CommandLineTool`. The conformance is dependent on the used `TaskBackend`. Currently there are the following existing and planned backends:
| Backend | Status | Overall Conformance |
|---------|--------|---------------------|
| Docker* |   ✔️  | ![all]( https://img.shields.io/badge/all-99%25-yellow ) ![required]( https://img.shields.io/badge/required-97%25-red ) ![command_line_tool]( https://img.shields.io/badge/command_line_tool-98%25-yellow ) ![expression_tool]( https://img.shields.io/badge/expression_tool-100%25-green )![workflow]( https://img.shields.io/badge/workflow-100%25-green )|
| Local   |   🏗️  | ![all]( https://img.shields.io/badge/all-99%25-yellow ) ![required]( https://img.shields.io/badge/required-97%25-red ) ![command_line_tool]( https://img.shields.io/badge/command_line_tool-98%25-yellow ) ![expression_tool]( https://img.shields.io/badge/expression_tool-100%25-green )![workflow]( https://img.shields.io/badge/workflow-99%25-yellow)   |
| TES     |   🧾  | -   |
| Slurm   |   🧾  | -   |

✔️: Fully operational - 🏗️: Under Construction - 🧾: Planned

*=Uses Docker even if DockerRequirement is not specified.

## License
This work is dual-licensed under Apache 2.0 and MIT . You can choose between one of them if you use this work. 

SPDX-License-Identifier: `Apache-2.0 OR MIT`

**Funded by**


![dfg](https://raw.githubusercontent.com/fairagro/sciwin/refs/heads/main/docs/src/assets/dfg.png)

DFG project number 501899475