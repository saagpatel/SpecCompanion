#!/usr/bin/env bash
set -eu

if [[ $# -lt 5 || $# -gt 6 ]]; then
  echo "usage: score-ci.sh <repo-root> <commit> <test-output> <artifact-root> <diagnostics-root> [cargo]" >&2
  exit 64
fi
repo_root=$1
commit=$2
results=$3
artifact_root=$4
diagnostics_root=$5
cargo_bin=${6:-cargo}

# Requiring new directories prevents a previous run from becoming score evidence.
if [[ -e "$artifact_root" || -e "$diagnostics_root" ]]; then
  echo 'artifact and diagnostics paths must be fresh directories' >&2
  exit 1
fi
mkdir -p "$artifact_root/score/spec" "$diagnostics_root"
if [[ -f "$results" ]]; then
  cp "$results" "$diagnostics_root/test-output.txt"
else
  printf 'No test output was produced.\n' > "$diagnostics_root/test-output.txt"
fi

spec_source="$repo_root/living-spec/living-spec"
score_dir="$artifact_root/score/spec"
for file in spec.md annotations.json; do
  if [[ ! -f "$spec_source/$file" ]]; then
    printf 'Missing required score input: %s\n' "$spec_source/$file" > "$diagnostics_root/score.log"
    echo 'score_ready=false' >> "${GITHUB_OUTPUT:-/dev/null}"
    exit 1
  fi
  cp "$spec_source/$file" "$score_dir/$file"
done
# History is useful transition context; status and HTML are deliberately never copied.
if [[ -f "$spec_source/history.jsonl" ]]; then
  cp "$spec_source/history.jsonl" "$score_dir/history.jsonl"
fi

set +e
(
  cd "$repo_root"
  "$cargo_bin" run --locked --manifest-path living-spec/Cargo.toml -- score "$score_dir" \
    --results "$results" --format libtest --tests living-spec --commit "$commit" \
    --html "$artifact_root/score/spec.html"
) > "$diagnostics_root/score.log" 2>&1
score_exit=$?
set -e

fresh=false
status_file="$score_dir/status.json"
if [[ ( "$score_exit" -eq 0 || "$score_exit" -eq 2 ) && -f "$status_file" && -s "$artifact_root/score/spec.html" ]] && ruby -rjson -e '
  begin
    status = JSON.parse(File.read(ARGV[0]))
    exit(status["commit"] == ARGV[1] ? 0 : 1)
  rescue StandardError
    exit 1
  end
' "$status_file" "$commit"; then
  fresh=true
fi
printf 'score_ready=%s\n' "$fresh" >> "${GITHUB_OUTPUT:-/dev/null}"

# Exit 2 represents a valid score containing broken requirements; keep it failing.
if [[ ( "$score_exit" -eq 0 || "$score_exit" -eq 2 ) && "$fresh" != true ]]; then
  echo 'scoring completed without emitting status and HTML for the attempted commit' >&2
  exit 1
fi
exit "$score_exit"
