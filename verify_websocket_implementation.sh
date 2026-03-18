#!/bin/bash
# Verify WebSocket Server Implementation
# This script verifies that all components are in place

set -e

echo "Verifying WebSocket server implementation..."
echo ""

# Check documentation
echo "✓ Checking documentation..."
test -f docs/docs/api-reference/ws-server.md || { echo "✗ Missing ws-server.md"; exit 1; }
echo "  - ws-server.md exists ($(wc -l < docs/docs/api-reference/ws-server.md) lines)"

# Check test files
echo "✓ Checking test files..."
test -f tests/websocket/test_echo.lua || { echo "✗ Missing test_echo.lua"; exit 1; }
test -f tests/websocket/test_chat.lua || { echo "✗ Missing test_chat.lua"; exit 1; }
test -f tests/websocket/test_multi_room.lua || { echo "✗ Missing test_multi_room.lua"; exit 1; }
test -f tests/websocket/test_websocket_servers.sh || { echo "✗ Missing test script"; exit 1; }
test -f tests/websocket/README.md || { echo "✗ Missing test README"; exit 1; }
echo "  - All Lua test files present"

# Check integration tests
echo "✓ Checking integration tests..."
test -f rover-server/tests/websocket_integration_test.rs || { echo "✗ Missing integration test"; exit 1; }
echo "  - Integration test file present"

# Check dependencies
echo "✓ Checking dependencies..."
grep -q "tokio-tungstenite" rover-server/Cargo.toml || { echo "✗ Missing tokio-tungstenite"; exit 1; }
echo "  - Test dependencies configured"

# Check documentation structure
echo "✓ Checking documentation structure..."
grep -q "## Lifecycle Hooks" docs/docs/api-reference/ws-server.md || { echo "✗ Missing Lifecycle Hooks section"; exit 1; }
grep -q "## Event Dispatch" docs/docs/api-reference/ws-server.md || { echo "✗ Missing Event Dispatch section"; exit 1; }
grep -q "## Error Handling" docs/docs/api-reference/ws-server.md || { echo "✗ Missing Error Handling section"; exit 1; }
echo "  - All required sections present"

# Check examples
echo "✓ Checking examples..."
test -f examples/websocket/echo.lua || { echo "✗ Missing echo.lua"; exit 1; }
test -f examples/websocket/chat.lua || { echo "✗ Missing chat.lua"; exit 1; }
test -f examples/websocket/multi_room_chat.lua || { echo "✗ Missing multi_room_chat.lua"; exit 1; }
echo "  - All example files present"

echo ""
echo "✅ All checks passed!"
echo ""
echo "Implementation summary:"
echo "  - Documentation: docs/docs/api-reference/ws-server.md"
echo "  - Tests: tests/websocket/*.lua"
echo "  - Integration tests: rover-server/tests/websocket_integration_test.rs"
echo "  - Examples: examples/websocket/*.lua"
