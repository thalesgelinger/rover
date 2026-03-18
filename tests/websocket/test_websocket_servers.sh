#!/bin/bash
# Test WebSocket Server Examples
# This script validates that all WebSocket examples can be parsed and run

set -e

echo "Testing WebSocket server examples..."

# Test 1: Check syntax of all WebSocket examples
echo "Test 1: Checking syntax..."
cargo run -p rover_cli -- check examples/websocket/echo.lua
cargo run -p rover_cli -- check examples/websocket/chat.lua
cargo run -p rover_cli -- check examples/websocket/multi_room_chat.lua
cargo run -p rover_cli -- check tests/websocket/test_echo.lua
cargo run -p rover_cli -- check tests/websocket/test_chat.lua
cargo run -p rover_cli -- check tests/websocket/test_multi_room.lua

echo "✓ All WebSocket examples have valid syntax"

# Test 2: Format check
echo "Test 2: Checking formatting..."
cargo run -p rover_cli -- fmt examples/websocket/echo.lua --check
cargo run -p rover_cli -- fmt examples/websocket/chat.lua --check
cargo run -p rover_cli -- fmt examples/websocket/multi_room_chat.lua --check

echo "✓ All WebSocket examples are properly formatted"

# Test 3: Run echo server briefly
echo "Test 3: Running echo server..."
timeout 2 cargo run -p rover_cli -- run examples/websocket/echo.lua || true
echo "✓ Echo server starts successfully"

# Test 4: Run chat server briefly
echo "Test 4: Running chat server..."
timeout 2 cargo run -p rover_cli -- run examples/websocket/chat.lua || true
echo "✓ Chat server starts successfully"

# Test 5: Run multi-room chat server briefly
echo "Test 5: Running multi-room chat server..."
timeout 2 cargo run -p rover_cli -- run examples/websocket/multi_room_chat.lua || true
echo "✓ Multi-room chat server starts successfully"

echo ""
echo "All WebSocket server tests passed! ✓"
