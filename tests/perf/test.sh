#!/bin/bash
set -euo pipefail

# Rust-native perf regression test.
# Override thresholds via env vars:
#   ROVER_PERF_THREADS
#   ROVER_PERF_REQUESTS_PER_THREAD
#   ROVER_PERF_MIN_RPS
#   ROVER_PERF_MAX_P99_MS

cargo test --release -p rover_cli perf_http_echo_regression -- --ignored --nocapture
