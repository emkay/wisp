#!/usr/bin/env bash
# Build and package Wisp for WASM deployment

set -e

echo "Building WASM (release)..."
cargo build --release --target wasm32-unknown-unknown

echo "Copying files to web/..."
cp target/wasm32-unknown-unknown/release/wisp.wasm web/

# Find and copy macroquad JS bundle if not present
if [ ! -f "web/mq_js_bundle.js" ]; then
    echo "Copying mq_js_bundle.js..."
    MQ_JS=$(find ~/.cargo/registry/src -name "mq_js_bundle.js" -path "*/macroquad-*" 2>/dev/null | head -1)
    if [ -n "$MQ_JS" ]; then
        cp "$MQ_JS" web/
    else
        echo "Warning: mq_js_bundle.js not found. You may need to copy it manually."
    fi
fi

echo ""
echo "Build complete!"
echo ""
echo "To run locally:"
echo "  cd web && npx serve"
echo "To specify a different game:"
echo "  Create web/wisp.conf with the game path"
echo "  Default: game.wisp"
echo ""
echo "Files in web/:"
ls -la web/
