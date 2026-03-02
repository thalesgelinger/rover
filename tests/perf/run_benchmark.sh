#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${GREEN}=== ROVER RUST PERF SUITE ===${NC}"
echo ""
echo -e "${YELLOW}Running perf regression integration test (release)...${NC}"
cd "$PROJECT_ROOT"
bash "$SCRIPT_DIR/test.sh"
echo ""
echo -e "${GREEN}✓ Perf regression test passed${NC}"
