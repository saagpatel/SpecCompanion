#!/bin/sh
# POSIX tools only. Check direct paths as well as multiline grouped crate imports.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)

for layer in core adapters agents runner boundary deps quantifier web pipeline cli; do
    case "$layer" in
        core) allowed='core' ;;
        adapters) allowed='core adapters' ;;
        agents) allowed='core adapters agents' ;;
        runner) allowed='core adapters runner' ;;
        boundary) allowed='core adapters boundary' ;;
        deps) allowed='core deps' ;;
        quantifier) allowed='core quantifier' ;;
        web) allowed='core web' ;;
        pipeline) allowed='core adapters agents runner boundary deps quantifier web pipeline' ;;
        cli) allowed='core adapters agents runner boundary deps quantifier web pipeline cli' ;;
    esac
    find "$root/src/$layer" -name '*.rs' -type f -exec grep -l -E 'crate[[:space:]]*::' {} + |
    while IFS= read -r source; do
        awk -v allowed="$allowed" '
        function check(module) {
            if (module != "" && index(" " allowed " ", " " module " ") == 0) {
                printf "%s:%d: forbidden layer import crate::%s\n", FILENAME, FNR, module
                failed = 1
            }
        }
        function paths(text, tail, module, depth, token) {
            while (match(text, /crate[[:space:]]*::[[:space:]]*/)) {
                text = substr(text, RSTART + RLENGTH)
                if (text ~ /^[A-Za-z_]/) {
                    match(text, /^[A-Za-z_][A-Za-z_0-9]*/)
                    check(substr(text, RSTART, RLENGTH))
                } else if (text ~ /^\{/) {
                    tail = substr(text, 2)
                    depth = 1
                    module = ""
                    while (length(tail) && depth) {
                        if (match(tail, /^[A-Za-z_][A-Za-z_0-9]*/)) {
                            token = substr(tail, RSTART, RLENGTH)
                            if (depth == 1 && module == "") {
                                check(token)
                                module = token
                            }
                            tail = substr(tail, RLENGTH + 1)
                        } else {
                            token = substr(tail, 1, 1)
                            if (token == "{") depth++
                            if (token == "}") depth--
                            if (token == "," && depth == 1) module = ""
                            tail = substr(tail, 2)
                        }
                    }
                }
            }
        }
        {
            # Accumulate multiline grouped uses before checking their roots.
            if (pending != "") {
                pending = pending " " $0
                if ($0 ~ /;/) { paths(pending); pending = "" }
            } else if ($0 ~ /use[[:space:]]+crate[[:space:]]*::/ && $0 !~ /;/) {
                pending = $0
            } else if ($0 ~ /crate[[:space:]]*::/) {
                paths($0)
            }
        }
        END { if (pending != "") paths(pending); exit failed }
        ' "$source"
    done
done
echo 'Module layers are valid.'
