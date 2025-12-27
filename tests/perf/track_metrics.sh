#!/bin/bash

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

if grep -q "SUMMARY:" "$RESULT_FILE"; then
    RPS=$(grep "requests_per_sec=" "$RESULT_FILE" | awk -F'=' '{print $2}')
    ERRORS=$(grep "total_errors=" "$RESULT_FILE" | awk -F'=' '{print $2}')
    P99=$(grep "p99=" "$RESULT_FILE" | grep "LATENCY_MS" -A 20 | grep "p99=" | head -1 | awk -F'=' '{print $2}')
    MEAN=$(grep "mean=" "$RESULT_FILE" | grep "LATENCY_MS" -A 10 | head -1 | awk -F'=' '{print $2}')

    echo "Requests/sec:  $RPS"
    echo "Mean Latency:  ${MEAN} ms"
    echo "P99 Latency:   ${P99} ms"
    echo "Total Errors:  $ERRORS"
    echo ""

    # Compare with previous run
    PREV_FILE=$(ls -t "$RESULTS_DIR"/benchmark_*.txt 2>/dev/null | head -2 | tail -1)
    if [ -n "$PREV_FILE" ] && [ "$PREV_FILE" != "$RESULT_FILE" ]; then
        echo -e "${YELLOW}=== COMPARISON WITH PREVIOUS RUN ===${NC}"
        PREV_RPS=$(grep "requests_per_sec=" "$PREV_FILE" | awk -F'=' '{print $2}')
        PREV_P99=$(grep "p99=" "$PREV_FILE" | grep "LATENCY_MS" -A 20 | grep "p99=" | head -1 | awk -F'=' '{print $2}')

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
