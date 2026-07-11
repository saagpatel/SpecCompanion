#!/usr/bin/env bash
set -euo pipefail

# codex-os-managed
branch="${GITHUB_HEAD_REF:-$(git rev-parse --abbrev-ref HEAD)}"

if [[ "$branch" == "HEAD" && -n "${GITHUB_REF_NAME:-}" ]]; then
  branch="$GITHUB_REF_NAME"
fi
typed_pattern='^codex/(feat|fix|chore|refactor|docs|test|perf|ci|spike|hotfix)/[a-z0-9]+(-[a-z0-9]+)*$'
peer_pattern='^(codex|cc)/[a-z0-9]+(-[a-z0-9]+)+$'

if [[ "$branch" == "main" || "$branch" == "master" ]]; then
  echo "Direct work on $branch is blocked."
  exit 1
fi

if [[ "$branch" == dependabot/* ]]; then
  exit 0
fi

if ! [[ "$branch" =~ $typed_pattern || "$branch" =~ $peer_pattern ]]; then
  echo "Invalid branch: $branch"
  echo "Expected: codex/<type>/<slug> or <agent>/<task-slug>"
  exit 1
fi
