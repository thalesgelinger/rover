#!/bin/bash
# Performance benchmark test
# -t2: 2 threads (2 CPUs)
# -c100: 100 concurrent connections
# -d30s: 30 second test duration
# -s benchmark.lua: Use custom benchmark script for detailed metrics

wrk -t2 -c100 -d30s -s benchmark.lua http://localhost:3000/echo

