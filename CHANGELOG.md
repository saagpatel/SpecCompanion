# Changelog

## [Unreleased]

- Replace pass-equals-coverage reporting with evidence-backed `VERIFIED`, `PARTIAL`, `FAILED`, and `UNKNOWN` classifications.
- Add stable source-linked requirements, placeholder assertion rejection, deterministic JavaScript/TypeScript and Python evidence, and bounded test execution.
- Add exact evidence trails and explanatory accessible report states.
- Add explicit contained repository-test linkage and bounded Vitest/unittest execution, proven against real TypeScript and Python projects on disposable copies.

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.0.0] - 2026-03-21

### Added

- Initial implementation of Spec Companion
- Deterministic UI gates and blocking LCP checks
- Lean mode and cleanup workflows
- Comprehensive unit tests for scanner, alignment, and hooks

### Fixed

- Critical bugs found during comprehensive code review
- Lower-priority issues and audit findings across backend and frontend
- Security hardening, transactions, and frontend bugs
- Vulnerable Rust transitive crates patched
- Tauri lockfile refreshed to latest compatible patches
- Visual baselines made cross-platform
- Mobile snapshot tolerance stabilized
- Cross-OS visual test flake reduced
- UI visual tolerances tuned for Linux CI

### Changed

- Replaced template README with comprehensive app documentation
- Improved scanner language parity and added session audit artifacts
- Approved pnpm build scripts for esbuild
- Added one-command project cleanup script
- Removed non-runtime bloat and unused tooling
- Added coverage directory to .gitignore
- Finalized codex OS bootstrap baseline
- Prepared GitHub baseline for CI
