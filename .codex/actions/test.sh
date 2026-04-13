#!/usr/bin/env bash
set -euo pipefail

# Codex artifact routing defaults (v6.0)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [ -f "$SCRIPT_DIR/_artifact_env.sh" ]; then
  # shellcheck source=/dev/null
  source "$SCRIPT_DIR/_artifact_env.sh"
fi

echo "NOT_RUN: No test script is documented in README.md or package.json for this repo."
echo "Add a canonical test command before treating production-ready quality gates as passing."

if [ "${CODEX_ALLOW_NOT_RUN_GATES:-0}" = "1" ]; then
  echo "Bypass enabled via CODEX_ALLOW_NOT_RUN_GATES=1; continuing with documented risk."
  exit 0
fi

exit 3
