# Rover Performance Benchmarking Suite

A comprehensive performance testing framework for Rover using `wrk` (HTTP benchmarking tool).

## Quick Start

Run the complete benchmark suite:
```bash
bash run_benchmark.sh
```

This will:
1. Build Rover in release mode
2. Start an echo server on port 3000
3. Run the benchmark with 2 threads, 100 connections, 30 seconds
4. Display detailed performance metrics in an AI-friendly format
5. Stop the server

## Files

- **main.lua** - Simple echo server with GET and POST `/echo` endpoints
- **benchmark.lua** - wrk Lua script with detailed metrics collection
- **test.sh** - Basic wrk command (2 threads, 100 connections, 30s)
- **run_benchmark.sh** - Complete automated benchmark suite
- **track_metrics.sh** - Save results and compare with previous runs
- **analyze.sh** - Performance analysis guide and bottleneck detection

## Benchmark Metrics Explained

### Summary Section
- **requests_per_sec**: Throughput - how many requests completed per second
- **total_requests**: Total number of requests completed in the test
- **success_rate**: Percentage of successful requests (100% = no errors)
- **throughput_mb_per_sec**: Network throughput in megabytes per second

### Latency Section (Critical for identifying bottlenecks)
- **min**: Best-case response time
- **mean**: Average response time
- **stdev**: Standard deviation (consistency - low is better)
- **max**: Worst-case response time
- **p50-p99**: Percentiles showing tail latency
  - **p50** (median): Half of requests are faster than this
  - **p99**: 99% of requests are faster than this - indicates tail performance
  - **p99.9**: 99.9% of requests - even more extreme outliers

### Error Section
Breakdown of what went wrong:
- **connect**: Failed to establish connection
- **read**: Failed reading response
- **write**: Failed sending request
- **status**: HTTP errors (4xx, 5xx)
- **timeout**: Request took too long

## Interpreting Results

### Good Performance Indicators
```
requests_per_sec=21958
mean=2.42ms
p99=5.22ms
success_rate=100.00%
errors=0
```
✓ High throughput, consistent latency, no errors

### Performance Issues
```
p99=50.00ms (vs mean=2.42ms)
```
→ Indicates occasional slowness - possible GC pauses or context switches

```
max=500ms
```
→ Check for resource limit hits or network issues

```
total_errors > 0
```
→ Server is overloaded or has stability issues

## Identifying Bottlenecks

### Method 1: Compare Results
```bash
# Run baseline
bash track_metrics.sh > baseline.txt

# Make code change
cargo build --release

# Run again
bash track_metrics.sh > after_change.txt

# Compare
diff baseline.txt after_change.txt
```

### Method 2: Use perf (Linux)
While benchmark is running:
```bash
# Terminal 1: Start benchmark
bash run_benchmark.sh

# Terminal 2: Profile
perf record -p <ROVER_PID> -g -F 99 sleep 30
perf report

# Or generate flame graph
perf script | inferno-collapse-perf | inferno-flamegraph > flame.svg
```

### Method 3: Monitor System
```bash
# Terminal 1: Run benchmark
bash run_benchmark.sh

# Terminal 2: Watch system
top -p <ROVER_PID>
```

Look for:
- CPU usage (should be near 100% during benchmark)
- Memory growth (should be stable)
- Context switches (high switches = contention)

## Tuning the Benchmark

Edit `test.sh` to change parameters:

```bash
# More threads (simulate more cores)
wrk -t4 -c100 -d30s ...  # 4 threads instead of 2

# More connections (more concurrent load)
wrk -t2 -c500 -d30s ...  # 500 connections instead of 100

# Longer duration (more stable results)
wrk -t2 -c100 -d60s ...  # 60 seconds instead of 30

# POST requests (different workload)
wrk -t2 -c100 -d30s -m POST -d '{"test": "data"}' ...
```

## What to Measure for Optimization

### 1. Identify the Bottleneck
- Run the benchmark and identify if issue is throughput or latency
- Use perf/flame graphs to find which functions are taking time

### 2. High-Value Optimization Areas in Rover
- **Lua API bindings** (`rover_core/src/bindings/`) - Called on every request
- **JSON serialization** - Can be pre-serialized for common responses
- **HTTP parsing** - Core request handling loop
- **Event loop scheduling** - Threading/async coordination
- **Memory allocations** - Reduce Box/Vec/String allocations in hot paths

### 3. Benchmark Results to Track
```
BEFORE optimization:  21958 RPS, p99=5.22ms
AFTER optimization:   25000 RPS, p99=4.50ms
Improvement: +14% throughput, -13% latency
```

## Performance Tips

1. **Use release builds** - Always test with `--release`
2. **Warm up first** - First few seconds of benchmark may be slower
3. **Multiple runs** - Run multiple times to catch variance
4. **Consistent environment** - Close other apps, consistent CPU governor
5. **Measure the right thing** - p99 latency matters more than average for users

## Common Bottlenecks & Fixes

### Problem: Low RPS, High CPU
→ Check if event loop is blocking (e.g., synchronous I/O)
→ Profile to find the hot function

### Problem: High Tail Latency (p99 >> mean)
→ Indicates occasional slowness
→ Could be: garbage collection pauses, thread context switches, resource exhaustion

### Problem: Memory growth
→ Possible memory leak in Lua integration
→ Check for unbounded tables/arrays

### Problem: Errors under load
→ Socket limit: `ulimit -n` (increase if needed)
→ Check server logs for actual error messages

## Results Storage

Benchmark results are stored in `results/` directory:
```bash
results/benchmark_2024-01-15_10-30-45.txt
results/benchmark_2024-01-15_11-15-22.txt
```

This allows you to:
- Track performance over time
- Compare between commits
- Detect regressions early

## Next Steps

1. Run baseline: `bash run_benchmark.sh`
2. Review results - note key metrics (RPS, p99 latency)
3. Identify slowest function: `bash analyze.sh`
4. Profile with perf: `perf record -p <PID>`
5. Optimize the bottleneck
6. Re-benchmark and compare results

## References

- wrk documentation: https://github.com/wg/wrk
- Linux perf: https://perf.wiki.kernel.org/
- Flame graphs: https://www.brendangregg.com/flamegraphs.html
