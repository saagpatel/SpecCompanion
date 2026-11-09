#!/usr/bin/env bash
set -eu

if [[ $# -lt 2 || $# -gt 3 ]]; then
  echo "usage: refresh-dogfood.sh <repo-root> <test-output> [cargo]" >&2
  exit 64
fi
repo_root=$1
results=$2
cargo_bin=${3:-cargo}
spec_source="$repo_root/living-spec/living-spec"
target_root="$repo_root/living-spec/target"
mkdir -p "$target_root"
stage=$(mktemp -d "$target_root/dogfood-refresh.XXXXXX")
cleanup() {
  rm -rf "$stage"
  for file in annotations.json status.json history.jsonl spec.html; do
    rm -f "$spec_source/.$file.refresh-$$"
  done
}
trap cleanup EXIT HUP INT TERM

cp -R "$spec_source/." "$stage/spec"
(
  cd "$repo_root"
  "$cargo_bin" run --offline --locked -q --manifest-path living-spec/Cargo.toml \
    -- score "$stage/spec" --results "$results" --format libtest --tests living-spec \
    --html "$stage/spec.html"
)

# The staged status moves to the source directory; keep its result locator
# relative to that final location. Paths do not participate in input digests.
ruby -rjson -rpathname -e '
  path, destination, results = ARGV
  status = JSON.parse(File.read(path))
  if status["inputs"]
    status["inputs"]["results_file"] = Pathname.new(File.expand_path(results)).relative_path_from(Pathname.new(File.expand_path(destination))).to_s
    File.write(path, JSON.pretty_generate(status) + "\n")
  end
' "$stage/spec/status.json" "$spec_source" "$results"

# Prepare every destination beside its final path before replacing any artifact.
for file in annotations.json status.json history.jsonl; do
  cp "$stage/spec/$file" "$spec_source/.$file.refresh-$$"
done
cp "$stage/spec.html" "$spec_source/.spec.html.refresh-$$"
mv -f "$spec_source/.annotations.json.refresh-$$" "$spec_source/annotations.json"
mv -f "$spec_source/.status.json.refresh-$$" "$spec_source/status.json"
mv -f "$spec_source/.history.jsonl.refresh-$$" "$spec_source/history.jsonl"
mv -f "$spec_source/.spec.html.refresh-$$" "$spec_source/spec.html"
