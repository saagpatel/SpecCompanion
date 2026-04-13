#!/usr/bin/env bash
set -euo pipefail

# codex-os-managed
max_bytes="${ASSET_MAX_BYTES:-450000}"
mkdir -p .perf-results
result_file=".perf-results/assets.json"
fail=0
checked=0

check_dir() {
  local dir="$1"
  [[ -d "$dir" ]] || return 0
  while IFS= read -r file; do
    [[ -n "$file" ]] || continue
    checked=$((checked + 1))
    size=$(wc -c < "$file")
    if (( size > max_bytes )); then
      echo "Asset too large (>${max_bytes} bytes): $file"
      fail=1
    fi
  done < <(find "$dir" -type f \( -name "*.png" -o -name "*.jpg" -o -name "*.jpeg" -o -name "*.webp" -o -name "*.avif" -o -name "*.svg" -o -name "*.gif" -o -name "*.ico" -o -name "*.css" -o -name "*.js" -o -name "*.mjs" -o -name "*.wasm" \))
}

check_dir dist/assets
check_dir public

cat > "$result_file" <<JSON
{
  "checked": $checked,
  "maxBytes": $max_bytes,
  "status": "$([[ $fail -eq 0 ]] && echo pass || echo fail)"
}
JSON

exit $fail
