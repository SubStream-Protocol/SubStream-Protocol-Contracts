#!/bin/bash
# Exit immediately if a command exits with a non-zero status
set -e

echo "🔨 Building release WASM..."
cargo build --target wasm32-unknown-unknown --release

# Find the generated WASM file
WASM_FILE=$(find target/wasm32-unknown-unknown/release -maxdepth 1 -name "*.wasm" | grep -v "_opt" | head -n 1)
if [ -z "$WASM_FILE" ]; then
    echo "❌ Error: Could not find compiled WASM file in target directory."
    exit 1
fi

# Define the output optimized file name
OPT_WASM_FILE="${WASM_FILE%.wasm}_opt.wasm"

# Check for wasm-opt dependency
if ! command -v wasm-opt &> /dev/null; then
    echo "⚠️ wasm-opt could not be found."
    echo "Installing via npm (requires Node.js)..."
    npm install -g wasm-opt
fi

echo "🗜️ Optimizing WASM with wasm-opt..."
# -Oz: aggressive size optimization
# --signext-lowering: required for Soroban compatibility
wasm-opt -Oz "$WASM_FILE" -o "$OPT_WASM_FILE"

# Calculate and display savings
ORIGINAL_SIZE=$(wc -c <"$WASM_FILE")
OPT_SIZE=$(wc -c <"$OPT_WASM_FILE")
SAVINGS=$(( ORIGINAL_SIZE - OPT_SIZE ))
PERCENTAGE=$(( SAVINGS * 100 / ORIGINAL_SIZE ))

echo "=========================================="
echo "✅ WASM Compression Complete!"
echo "Original Size:  $ORIGINAL_SIZE bytes"
echo "Optimized Size: $OPT_SIZE bytes"
echo "Space Saved:    $SAVINGS bytes ($PERCENTAGE%)"
echo "Output File:    $OPT_WASM_FILE"
echo "=========================================="

# Check 64KB Soroban limit guardrail
if [ "$OPT_SIZE" -gt 65536 ]; then
    echo "⚠️ WARNING: Optimized WASM is still over the 64KB Soroban deployment limit!"
fi