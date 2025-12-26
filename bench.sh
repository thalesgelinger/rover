#!/bin/bash

# Rover Server Benchmark Suite
# Comprehensive automated performance testing for JSON serialization,
# request handling, cloning, and end-to-end server performance

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Default configuration
BENCHMARK_TYPE="all"
PROFILE="release"
DURATION="30s"
THREADS=4
CONNECTIONS=100
SKIP_BUILD=false
OPEN_REPORT=false

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -t|--type)
            BENCHMARK_TYPE="$2"
            shift 2
            ;;
        -p|--profile)
            PROFILE="$2"
            shift 2
            ;;
        -d|--duration)
            DURATION="$2"
            shift 2
            ;;
        --threads)
            THREADS="$2"
            shift 2
            ;;
        --connections)
            CONNECTIONS="$2"
            shift 2
            ;;
        --skip-build)
            SKIP_BUILD=true
            shift
            ;;
        --open)
            OPEN_REPORT=true
            shift
            ;;
        -h|--help)
            echo "Rover Server Benchmark Suite"
            echo ""
            echo "Usage: ./bench.sh [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  -t, --type TYPE        Benchmark type: all, micro, server, json, cloning (default: all)"
            echo "  -p, --profile PROFILE  Build profile: release, dev (default: release)"
            echo "  -d, --duration TIME    Server benchmark duration (default: 30s)"
            echo "  --threads NUM          wrk threads (default: 4)"
            echo "  --connections NUM      wrk concurrent connections (default: 100)"
            echo "  --skip-build           Skip cargo build step"
            echo "  --open                 Open HTML report after benchmarks"
            echo "  -h, --help             Show this help message"
            echo ""
            echo "Benchmark types:"
            echo "  all      - Run all benchmarks (micro + server)"
            echo "  micro    - Run only Criterion micro-benchmarks (JSON + cloning)"
            echo "  server   - Run only wrk server benchmarks"
            echo "  json     - Run only JSON serialization benchmarks"
            echo "  cloning  - Run only request cloning benchmarks"
            echo ""
            echo "Examples:"
            echo "  ./bench.sh                                    # Run all benchmarks"
            echo "  ./bench.sh -t json                           # Run only JSON benchmarks"
            echo "  ./bench.sh -t server -d 60s --connections 500 # Server benchmarks with custom settings"
            echo "  ./bench.sh --skip-build --open               # Skip build and open report"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            echo "Use -h or --help for usage information"
            exit 1
            ;;
    esac
done

echo -e "${CYAN}╔════════════════════════════════════════════════════════════╗${NC}"
echo -e "${CYAN}║         Rover Server Benchmark Suite                      ║${NC}"
echo -e "${CYAN}╚════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "${BLUE}Configuration:${NC}"
echo -e "  Type:        $BENCHMARK_TYPE"
echo -e "  Profile:     $PROFILE"
echo -e "  Duration:    $DURATION"
echo -e "  Threads:     $THREADS"
echo -e "  Connections: $CONNECTIONS"
echo ""

# Build project if needed
if [ "$SKIP_BUILD" = false ]; then
    echo -e "${YELLOW}Building project in $PROFILE mode...${NC}"
    if [ "$PROFILE" = "release" ]; then
        cargo build --release
    else
        cargo build
    fi
    echo -e "${GREEN}✓ Build complete${NC}"
    echo ""
fi

# Function to run Criterion benchmarks
run_criterion_benchmarks() {
    local bench_name=$1
    local bench_title=$2

    echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${CYAN}  $bench_title${NC}"
    echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo ""

    cd rover_server
    if [ "$PROFILE" = "release" ]; then
        cargo bench --bench "$bench_name"
    else
        cargo bench --bench "$bench_name" --profile dev
    fi
    cd ..

    echo -e "${GREEN}✓ $bench_title complete${NC}"
    echo ""
}

# Function to run server benchmarks
run_server_benchmark() {
    local scenario=$1
    local wrk_script=$2
    local title=$3

    echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${CYAN}  $title${NC}"
    echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo ""

    # Start server in background
    echo -e "${YELLOW}Starting Rover server...${NC}"
    if [ "$PROFILE" = "release" ]; then
        ./target/release/rover tests/perf/scenarios/$scenario &
    else
        ./target/debug/rover tests/perf/scenarios/$scenario &
    fi
    SERVER_PID=$!

    # Wait for server to start
    sleep 2

    # Check if server is running
    if ! kill -0 $SERVER_PID 2>/dev/null; then
        echo -e "${RED}✗ Failed to start server${NC}"
        return 1
    fi

    echo -e "${GREEN}✓ Server started (PID: $SERVER_PID)${NC}"
    echo ""

    # Run wrk benchmark
    echo -e "${YELLOW}Running benchmark...${NC}"
    wrk -t$THREADS -c$CONNECTIONS -d$DURATION \
        -s tests/perf/wrk_scripts/$wrk_script \
        -s tests/perf/benchmark.lua \
        http://localhost:3000

    # Stop server
    echo ""
    echo -e "${YELLOW}Stopping server...${NC}"
    kill $SERVER_PID 2>/dev/null || true
    wait $SERVER_PID 2>/dev/null || true

    echo -e "${GREEN}✓ $title complete${NC}"
    echo ""
    sleep 1
}

# Run benchmarks based on type
case $BENCHMARK_TYPE in
    all)
        # Run micro-benchmarks
        run_criterion_benchmarks "json_serialization" "JSON Serialization Benchmarks"
        run_criterion_benchmarks "request_handling" "Request Handling & Cloning Benchmarks"

        # Run server benchmarks
        echo -e "${BLUE}Starting server benchmarks...${NC}"
        echo ""

        run_server_benchmark "simple_json.lua" "simple.lua" "Simple JSON Response"
        run_server_benchmark "complex_json.lua" "complex.lua" "Complex JSON Response (50 objects)"
        run_server_benchmark "with_params.lua" "params.lua" "URL Parameters Extraction"
        run_server_benchmark "with_headers.lua" "headers.lua" "Header Parsing"
        run_server_benchmark "with_query.lua" "query.lua" "Query String Parsing"
        ;;

    micro)
        run_criterion_benchmarks "json_serialization" "JSON Serialization Benchmarks"
        run_criterion_benchmarks "request_handling" "Request Handling & Cloning Benchmarks"
        ;;

    json)
        run_criterion_benchmarks "json_serialization" "JSON Serialization Benchmarks"
        ;;

    cloning)
        run_criterion_benchmarks "request_handling" "Request Handling & Cloning Benchmarks"
        ;;

    server)
        echo -e "${BLUE}Starting server benchmarks...${NC}"
        echo ""

        run_server_benchmark "simple_json.lua" "simple.lua" "Simple JSON Response"
        run_server_benchmark "complex_json.lua" "complex.lua" "Complex JSON Response (50 objects)"
        run_server_benchmark "with_params.lua" "params.lua" "URL Parameters Extraction"
        run_server_benchmark "with_headers.lua" "headers.lua" "Header Parsing"
        run_server_benchmark "with_query.lua" "query.lua" "Query String Parsing"
        ;;

    *)
        echo -e "${RED}Unknown benchmark type: $BENCHMARK_TYPE${NC}"
        echo "Use -h or --help for usage information"
        exit 1
        ;;
esac

echo ""
echo -e "${CYAN}╔════════════════════════════════════════════════════════════╗${NC}"
echo -e "${CYAN}║              Benchmarks Complete!                         ║${NC}"
echo -e "${CYAN}╚════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "${GREEN}Results:${NC}"

if [[ "$BENCHMARK_TYPE" == "all" || "$BENCHMARK_TYPE" == "micro" || "$BENCHMARK_TYPE" == "json" || "$BENCHMARK_TYPE" == "cloning" ]]; then
    echo -e "  • Criterion reports: ${BLUE}rover_server/target/criterion/${NC}"
    echo -e "  • HTML reports:      ${BLUE}rover_server/target/criterion/*/report/index.html${NC}"
fi

if [[ "$BENCHMARK_TYPE" == "all" || "$BENCHMARK_TYPE" == "server" ]]; then
    echo -e "  • Server benchmark results displayed above"
fi

echo ""

# Open HTML report if requested
if [ "$OPEN_REPORT" = true ]; then
    echo -e "${YELLOW}Opening Criterion HTML report...${NC}"
    # Find the most recent report
    REPORT=$(find rover_server/target/criterion -name "index.html" | head -n 1)
    if [ -n "$REPORT" ]; then
        if command -v xdg-open &> /dev/null; then
            xdg-open "$REPORT"
        elif command -v open &> /dev/null; then
            open "$REPORT"
        else
            echo -e "${YELLOW}Could not find a command to open the browser${NC}"
            echo -e "Report location: $REPORT"
        fi
    else
        echo -e "${RED}No Criterion HTML reports found${NC}"
    fi
fi

echo -e "${GREEN}Done!${NC}"
