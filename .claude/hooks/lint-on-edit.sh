#!/bin/bash
# Hook: Run cargo clippy on the edited file after Write/Edit events.
# Input: JSON on stdin with tool_input.file_path
# Exit 0: allow (lint output shown as context)
# Exit 2: block (would prevent the edit — we don't want that)

INPUT=$(cat)
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // empty')

# Only lint Rust files
if [[ "$FILE_PATH" != *.rs ]]; then
  exit 0
fi

# Check if cargo is available
if ! command -v cargo &> /dev/null; then
  exit 0
fi

# Run clippy on the workspace — capture output but don't block the edit
OUTPUT=$(cargo clippy --workspace --message-format=short 2>&1)
EXIT_CODE=$?

if [ $EXIT_CODE -ne 0 ]; then
  echo "Clippy warnings after editing $FILE_PATH:"
  echo "$OUTPUT" | grep -E "^(warning|error)" | head -20
  # Exit 0 — don't block, just inform
  exit 0
fi

echo "Clippy: clean"
exit 0
