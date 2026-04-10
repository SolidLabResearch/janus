#!/bin/bash

set -euo pipefail

cd "$(dirname "$0")/.."

failures=0

while IFS= read -r file; do
    while IFS= read -r raw_link; do
        link="${raw_link#<}"
        link="${link%>}"
        link="${link%%#*}"
        link="${link%% *}"

        if [[ -z "$link" ]]; then
            continue
        fi

        if [[ "$link" == http://* ]] ||
            [[ "$link" == https://* ]] ||
            [[ "$link" == mailto:* ]] ||
            [[ "$link" == \#* ]] ||
            [[ "$link" == app://* ]] ||
            [[ "$link" == plugin://* ]]; then
            continue
        fi

        target="$(cd "$(dirname "$file")" && pwd)/$link"
        if [[ ! -e "$target" ]]; then
            echo "Broken link in $file -> $raw_link"
            failures=1
        fi
    done < <(perl -ne 'while (/\[[^][]+\]\(([^)]+)\)/g) { print "$1\n"; }' "$file")
done < <(find . -path './target' -prune -o -name '*.md' -print)

if [[ "$failures" -ne 0 ]]; then
    echo "Documentation link check failed."
    exit 1
fi

echo "Documentation link check passed."
