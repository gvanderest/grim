#!/usr/bin/env bash
#
# Enforce module size caps (standard: .planning ticket 04/05, documented in
# AGENTS.md). clippy owns the per-function cap (`too_many_lines`); this script
# owns the per-file caps clippy has no lint for:
#
#   - production (non-test) code:      <= 400 lines per file
#   - shell files (lib.rs / mod.rs):   <=  80 lines  (doc + mod decls + re-exports)
#
# It ALSO enforces that shell files (lib.rs / mod.rs) contain **no definitions**
# — only module declarations and re-exports. Any `fn` / `struct` / `enum` /
# `trait` / `impl` / `union` / `type` / `const` / `static` / `macro_rules!` in a
# lib.rs or mod.rs is a violation (at any visibility — `pub`, `pub(crate)`, …):
# move it to a sibling concern module. Only the production region (before the
# first `#[cfg(test)]`) is scanned, so inline test modules are exempt.
#
# Test code is EXEMPT (tests are kept inline but split by concern; they do not
# count toward a file's size). A file's test code is:
#   - any file under a `tests/` directory (integration tests + harness), or
#   - a file named `tests.rs` (a split-out unit-test module), or
#   - every line from the first `#[cfg(test)]` onward in a production file.
#
# Production line count = lines before the first `#[cfg(test)]`, else the whole
# file. Exits non-zero (listing every violator) if any file is over its cap.
set -uo pipefail

REPO_ROOT=$(git rev-parse --show-toplevel)
cd "$REPO_ROOT" || exit 1

CAP_FILE=400
CAP_SHELL=80
fail=0

# Discover files up front. Run `find` into a temp file rather than a process
# substitution so its exit status is observable: a partial-traversal failure that
# still emitted some paths must abort, not silently check a truncated tree and
# report success.
list=$(mktemp)
trap 'rm -f "$list"' EXIT
if ! find crates -name '*.rs' -type f > "$list"; then
    echo "ERROR: find failed while discovering Rust files"
    exit 1
fi
mapfile -t files < "$list"
if (( ${#files[@]} == 0 )); then
    echo "ERROR: no Rust files found under crates/ (wrong cwd?)"
    exit 1
fi

for f in "${files[@]}"; do
    # Whole-file test code is exempt.
    case "$f" in
        */tests/*) continue ;;
        */tests.rs) continue ;;
    esac

    # Production lines = up to the first `#[cfg(test)]`, else the whole file.
    # Anchor the attribute at line start (allowing indentation) so a commented
    # `// #[cfg(test)]` does not falsely cut the production boundary early.
    prod=$(awk '/^[[:space:]]*#\[cfg\(test\)\]/{print NR-1; found=1; exit} END{if(!found) print NR}' "$f")

    base=$(basename "$f")
    if [[ "$base" == "lib.rs" || "$base" == "mod.rs" ]]; then
        cap=$CAP_SHELL
        kind="shell"
    else
        cap=$CAP_FILE
        kind="file"
    fi

    if (( prod > cap )); then
        echo "FAIL (${kind} cap ${cap}): ${f} has ${prod} production lines"
        fail=1
    fi

    # Shell files (lib.rs / mod.rs) must be declarations + re-exports only — no
    # definitions. Scan only the production region so inline test modules are
    # exempt. Match `fn`/`struct`/`enum`/`trait` (with optional pub/async) and
    # `impl` (with optional pub/unsafe) anchored at line start.
    if [[ "$kind" == "shell" && "$prod" -gt 0 ]]; then
        # `pub`, `pub(crate)`, `pub(super)`, `pub(in ...)` all count as a
        # definition's visibility — match the bare keyword or a `pub(...)` form.
        vis='(pub([[:space:]]+|\([^)]*\)[[:space:]]*))?'
        defs=$(head -n "$prod" "$f" | grep -nE \
            -e "^[[:space:]]*${vis}(async[[:space:]]+)?(const[[:space:]]+)?(unsafe[[:space:]]+)?(fn|struct|enum|trait|union|type|const|static)\b" \
            -e "^[[:space:]]*${vis}(unsafe[[:space:]]+)?impl\b" \
            -e '^[[:space:]]*macro_rules![[:space:]]*' || true)
        if [[ -n "$defs" ]]; then
            echo "FAIL (no defs in ${base}): ${f} contains definitions — move them to a sibling module:"
            echo "$defs" | sed 's/^/    /'
            fail=1
        fi
    fi
done

if (( fail )); then
    echo ""
    echo "size-cap check failed. Split oversized modules by concern (see AGENTS.md)."
    echo "Test code is exempt; shell files (lib.rs/mod.rs) must stay <= ${CAP_SHELL} lines."
    exit 1
fi

echo "size-cap check: all ${CAP_FILE}/${CAP_SHELL} caps satisfied"
