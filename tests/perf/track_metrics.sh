#!/bin/bash
set -euo pipefail

# Performance Metrics Tracker
# Runs benchmark and saves results for comparison and regression detection

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RESULTS_DIR="$SCRIPT_DIR/results"
TIMESTAMP=$(date +"%Y-%m-%d_%H-%M-%S")
RESULT_FILE="$RESULTS_DIR/benchmark_${TIMESTAMP}.txt"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

# Create results directory
mkdir -p "$RESULTS_DIR"

echo -e "${YELLOW}Running benchmark and saving results...${NC}"
echo "Results will be saved to: $RESULT_FILE"
echo ""

# Run benchmark and capture output
bash "$SCRIPT_DIR/run_benchmark.sh" | tee "$RESULT_FILE"

# Extract key metrics
echo ""
echo -e "${YELLOW}=== KEY METRICS ===${NC}"
echo ""

PERF_LINE=$(grep "perf: total=" "$RESULT_FILE" | tail -1 || true)
if [ -n "$PERF_LINE" ]; then
    RPS=$(echo "$PERF_LINE" | sed -E 's/.* rps=([0-9]+).*/\1/')
    ERRORS=$(echo "$PERF_LINE" | sed -E 's/.* errors=([0-9]+).*/\1/')
    MEAN_US=$(echo "$PERF_LINE" | sed -E 's/.* mean_us=([0-9]+).*/\1/')
    P99_US=$(echo "$PERF_LINE" | sed -E 's/.* p99_us=([0-9]+).*/\1/')
    MEAN=$(echo "scale=3; $MEAN_US/1000" | bc)
    P99=$(echo "scale=3; $P99_US/1000" | bc)

    echo "Requests/sec:  $RPS"
    echo "Mean Latency:  ${MEAN} ms"
    echo "P99 Latency:   ${P99} ms"
    echo "Total Errors:  $ERRORS"
    echo ""

    # Compare with previous run
    PREV_FILE=$(ls -t "$RESULTS_DIR"/benchmark_*.txt 2>/dev/null | head -2 | tail -1)
    if [ -n "$PREV_FILE" ] && [ "$PREV_FILE" != "$RESULT_FILE" ]; then
        echo -e "${YELLOW}=== COMPARISON WITH PREVIOUS RUN ===${NC}"
        PREV_PERF_LINE=$(grep "perf: total=" "$PREV_FILE" | tail -1 || true)
        PREV_RPS=$(echo "$PREV_PERF_LINE" | sed -E 's/.* rps=([0-9]+).*/\1/')
        PREV_P99_US=$(echo "$PREV_PERF_LINE" | sed -E 's/.* p99_us=([0-9]+).*/\1/')
        PREV_P99=$(echo "scale=3; $PREV_P99_US/1000" | bc)

        if [ -n "$PREV_RPS" ] && [ -n "$RPS" ]; then
            RPS_CHANGE=$(echo "scale=2; (($RPS - $PREV_RPS) / $PREV_RPS) * 100" | bc)
            if (( $(echo "$RPS_CHANGE > 0" | bc -l) )); then
                echo -e "${GREEN}RPS changed: +${RPS_CHANGE}%${NC} ($(printf "%.0f" $RPS) vs $(printf "%.0f" $PREV_RPS))"
            else
                echo -e "${RED}RPS changed: ${RPS_CHANGE}%${NC} ($(printf "%.0f" $RPS) vs $(printf "%.0f" $PREV_RPS))"
            fi
        fi

        if [ -n "$PREV_P99" ] && [ -n "$P99" ]; then
            P99_CHANGE=$(echo "scale=2; (($PREV_P99 - $P99) / $PREV_P99) * 100" | bc)
            if (( $(echo "$P99_CHANGE > 0" | bc -l) )); then
                echo -e "${GREEN}P99 Latency improved: +${P99_CHANGE}%${NC} (${P99}ms vs ${PREV_P99}ms)"
            else
                echo -e "${RED}P99 Latency regressed: ${P99_CHANGE}%${NC} (${P99}ms vs ${PREV_P99}ms)"
            fi
        fi
    fi
else
    echo -e "${RED}Could not extract metrics from benchmark output${NC}"
fi

echo ""
echo -e "${YELLOW}All benchmark results saved in: $RESULTS_DIR/${NC}"
echo "View previous runs: ls -la $RESULTS_DIR"
echo ""
