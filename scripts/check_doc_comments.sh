#!/usr/bin/env bash
# check_doc_comments.sh
#
# Fails the CI pipeline if any public function or struct in the Rust source
# files is missing a doc-comment (/// ...).
#
# Usage: ./scripts/check_doc_comments.sh
# Exit code: 0 = all public items documented, 1 = missing doc-comments found.

set -euo pipefail

SRC_DIR="${1:-contracts/substream_contracts/src}"
MISSING=0

# Patterns to match public items that require doc-comments.
PUB_ITEM_PATTERN='^\s*(pub(\([^)]*\))?\s+)?(fn|struct|enum|trait)\s'

while IFS= read -r -d '' file; do
    prev_line=""
    line_num=0
    while IFS= read -r line; do
        line_num=$((line_num + 1))
        # Check if this line declares a public item
        if echo "$line" | grep -qE '^\s+pub\s+(fn|struct|enum)\s'; then
            # The previous non-blank line must be a doc-comment
            if ! echo "$prev_line" | grep -qE '^\s*///'; then
                echo "MISSING DOC-COMMENT: $file:$line_num"
                echo "  -> $line"
                MISSING=$((MISSING + 1))
            fi
        fi
        # Track previous non-blank line
        if echo "$line" | grep -qE '\S'; then
            prev_line="$line"
        fi
    done < "$file"
done < <(find "$SRC_DIR" -name "lib.rs" -print0)

if [ "$MISSING" -gt 0 ]; then
    echo ""
    echo "ERROR: $MISSING public item(s) are missing doc-comments."
    echo "Add a '/// ...' doc-comment immediately above each flagged item."
    exit 1
else
    echo "OK: All public items in $SRC_DIR have doc-comments."
fi
