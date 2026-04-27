#!/usr/bin/env bash
# scripts/check_doc_comments.sh
#
# Fails the CI pipeline if any public function or struct in the Rust source
# lacks a doc-comment (/// or /** ... */).
#
# Usage:
#   ./scripts/check_doc_comments.sh [path/to/src]   (default: contracts/)
#
# Exit codes:
#   0 — all public items are documented
#   1 — one or more public items are missing doc-comments

set -euo pipefail

SRC_DIR="${1:-contracts}"
ERRORS=0

# Collect all .rs files (excluding test files and generated code)
mapfile -t RS_FILES < <(find "$SRC_DIR" -name "*.rs" \
  ! -name "test*.rs" \
  ! -path "*/fuzz/*" \
  ! -path "*/target/*")

for file in "${RS_FILES[@]}"; do
  line_num=0
  prev_line=""
  while IFS= read -r line; do
    line_num=$((line_num + 1))

    # Detect a public function or struct declaration
    if echo "$line" | grep -qE '^\s+pub fn |^pub fn |^\s+pub struct |^pub struct '; then
      # The previous non-empty line must be a doc-comment or a #[...] attribute
      # that itself follows a doc-comment. We check the immediate predecessor.
      stripped_prev=$(echo "$prev_line" | sed 's/^[[:space:]]*//')

      if ! echo "$stripped_prev" | grep -qE '^///|^\*\*|^#\['; then
        echo "MISSING DOC-COMMENT: $file:$line_num"
        echo "  -> $line"
        ERRORS=$((ERRORS + 1))
      fi
    fi

    # Track previous non-blank line
    if [ -n "$(echo "$line" | tr -d '[:space:]')" ]; then
      prev_line="$line"
    fi
  done < "$file"
done

if [ "$ERRORS" -gt 0 ]; then
  echo ""
  echo "ERROR: $ERRORS public item(s) are missing doc-comments."
  echo "Add a /// doc-comment above each flagged pub fn or pub struct."
  exit 1
fi

echo "OK: All public items have doc-comments."
