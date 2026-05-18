# Spec Companion

[![Rust](https://img.shields.io/badge/Rust-dea584?style=flat-square&logo=rust)](#) [![TypeScript](https://img.shields.io/badge/TypeScript-3178c6?style=flat-square&logo=typescript)](#) [![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](#)

> Specs drift from code. This closes the gap — automatically.

Spec Companion is a native desktop app that verifies your code actually implements what your spec says it should. Point it at a codebase and a markdown spec, let it extract requirements, generate tests, run them, and get a coverage report showing exactly which requirements have tests, which pass, and which are missing entirely.

Built with Tauri v2 (Rust backend, React frontend). Runs locally, works offline, keeps your code and specs private.

## Features

- **Requirement extraction** — parses markdown specs using `pulldown-cmark` AST (not regex), understands heading hierarchy, and classifies each requirement by type (functional, non-functional, constraint) and priority
- **Codebase scanning** — detects functions, classes, and methods across TypeScript, JavaScript, Python, Rust, Go, Java, Ruby, and C# for test generation context
- **Template test generation** — instant, offline Jest/PyTest skeletons with Arrange/Act/Assert structure and traceability comments linking back to requirements
- **LLM test generation** — optional Claude API mode generates tests with meaningful assertions, edge cases, and realistic mock data
- **Real test execution** — spawns actual `npx jest` or `python -m pytest` processes against your codebase; 120-second timeout, stdout/stderr captured without pipe deadlocks
- **Alignment reports** — coverage percentage plus categorized mismatches: No Test, Not Implemented, Test Failing, Partial Coverage
- **Export** — JSON, HTML, or CSV reports

## Quick Start

### Prerequisites

- Node.js 18+
- Rust stable toolchain (`rustup`)
- Tauri system dependencies: [tauri.app/start/prerequisites](https://tauri.app/start/prerequisites/)

### Installation

```bash
git clone https://github.com/saagpatel/SpecCompanion
cd SpecCompanion
npm install
```

### Usage

```bash
# Start in development mode
npm run tauri dev
```

1. Select your codebase directory and upload a markdown spec
2. Requirements are extracted automatically
3. Choose template mode (offline) or LLM mode (Claude API) to generate tests
4. Run tests against your codebase
5. Review the alignment report

## Tech Stack

| Layer               | Technology                                     |
| ------------------- | ---------------------------------------------- |
| Desktop shell       | Tauri 2                                        |
| Frontend            | React, TypeScript, Tailwind CSS                |
| Requirement parsing | Rust + `pulldown-cmark` (AST-based, not regex) |
| Test execution      | Rust subprocess runner (Jest, PyTest)          |
| LLM integration     | Anthropic Claude API (optional)                |
| Storage             | SQLite (local app data dir)                    |

## Architecture

The Rust backend owns the full pipeline: parse spec → scan codebase → generate tests → execute tests → compute alignment. The frontend is a step-by-step workflow UI that calls into Rust commands at each stage. All data stays local in SQLite. The only network call is the optional Claude API for LLM test generation, triggered explicitly by the user.

## License

MIT
