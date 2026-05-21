#!/usr/bin/env bash
# Frontend code quality gate.
# Exits non-zero if violations are found.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
FRONTEND_SRC="$PROJECT_ROOT/layream-app/src"

if [ ! -d "$FRONTEND_SRC" ]; then
    echo "SKIP: $FRONTEND_SRC not found"
    exit 0
fi

violations=0

# Rule 1: No catch (_) in .svelte/.js/.ts files
# catch(_) swallows the error without naming it -- a silent error concealment.
while IFS= read -r file; do
    while IFS=: read -r lineno line; do
        echo "VIOLATION (catch _): $file:$lineno: $line"
        violations=$((violations + 1))
    done < <(grep -n 'catch\s*(_)' "$file" 2>/dev/null || true)
done < <(find "$FRONTEND_SRC" \( -name "*.svelte" -o -name "*.js" -o -name "*.ts" \) -not -path "*/node_modules/*" -type f)

# Rule 2: No window.location.reload() in .svelte files
# Full page reload is almost always a workaround for state management issues.
while IFS= read -r file; do
    while IFS=: read -r lineno line; do
        echo "VIOLATION (reload): $file:$lineno: $line"
        violations=$((violations + 1))
    done < <(grep -n 'window\.location\.reload' "$file" 2>/dev/null || true)
done < <(find "$FRONTEND_SRC" -name "*.svelte" -not -path "*/node_modules/*" -type f)

if [ "$violations" -gt 0 ]; then
    echo ""
    echo "FAILED: $violations frontend quality violation(s) found."
    exit 1
else
    echo "OK: Frontend quality checks passed."
    exit 0
fi
