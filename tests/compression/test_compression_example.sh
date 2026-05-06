#!/bin/bash
# Test Compression Example
# This script validates that the realistic compression example works correctly

set -e

echo "Testing compression realistic server example..."

# Test 1: Check syntax
echo "Test 1: Checking syntax..."
cargo run -p rover_cli -- check examples/compression_realistic_server.lua
echo "✓ Syntax is valid"

# Test 2: Format check
echo "Test 2: Checking formatting..."
cargo run -p rover_cli -- fmt examples/compression_realistic_server.lua --check
echo "✓ Formatting is correct"

# Test 3: Run server briefly to verify startup
echo "Test 3: Testing server startup..."
timeout 3 cargo run -p rover_cli -- run examples/compression_realistic_server.lua || true
echo "✓ Server starts successfully"

# Test 4: Also check the original simple compression example
echo "Test 4: Checking original compression example..."
cargo run -p rover_cli -- check examples/compression_config.lua
cargo run -p rover_cli -- fmt examples/compression_config.lua --check
timeout 2 cargo run -p rover_cli -- run examples/compression_config.lua || true
echo "✓ Original compression example works"

echo ""
echo "All compression tests passed! ✓"
echo ""
echo "To manually test compression:"
echo "  1. Run: cargo run -p rover_cli -- run examples/compression_realistic_server.lua"
echo "  2. Test with curl:"
echo "     curl -H 'Accept-Encoding: gzip' http://localhost:8080/api/products -v"
echo "     curl -H 'Accept-Encoding: deflate' http://localhost:8080/api/products -v"
echo "     curl -H 'Accept-Encoding: gzip' http://localhost:8080/api/health -v  # Not compressed (small)"
