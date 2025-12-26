#!/bin/bash
set -e

cd "$(dirname "$0")/../.."

echo "🔥 Rover Performance Test"
echo "========================"
echo ""
echo "📊 Test Configuration:"
echo "   - Endpoint: GET /yabadabadoo"
echo "   - Response: Small JSON (api.json)"
echo "   - Threads: 4"
echo "   - Connections: 100"
echo "   - Duration: 30 seconds"
echo ""

# Start server in background
echo "🚀 Starting server..."
./target/release/rover tests/perf/main.lua > /dev/null 2>&1 &
SERVER_PID=$!

# Wait for server
echo "⏳ Waiting for server to be ready..."
sleep 2
for i in {1..10}; do
    if curl -s http://localhost:3000/yabadabadoo > /dev/null 2>&1; then
        echo "✅ Server is ready!"
        break
    fi
    sleep 1
done

echo ""
echo "🏃 Running benchmark..."
echo ""

cd tests/perf
wrk -t4 -c100 -d30s -s benchmark.lua http://localhost:3000

# Kill server
kill $SERVER_PID 2>/dev/null || true

echo ""
echo "✅ Test complete!"
