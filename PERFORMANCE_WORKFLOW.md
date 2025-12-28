# Performance Benchmarking Workflow for Rover Server

## Quick Reference

### Run Benchmark
```bash
cd tests/perf
./run_benchmark.sh
```

### Track Changes
```bash
cd tests/perf
./track_metrics.sh
```

### Profile and Find Bottlenecks
```bash
# Terminal 1: Start benchmark
cd tests/perf
./run_benchmark.sh

# Terminal 2: Profile (while benchmark runs)
ps aux | grep rover  # Get PID
perf record -p <PID> -g -F 99 sleep 30
perf report
```

## Systematic Optimization Process

### Phase 1: Establish Baseline

1. **Run clean benchmark:**
   ```bash
   cd tests/perf
   ./track_metrics.sh
   ```

2. **Record key metrics:**
   - Requests per second: `requests_per_sec`
   - P99 latency: `p99` (most important for user experience)
   - Success rate: `success_rate` (should be 100%)

### Phase 2: Profile and Identify Bottlenecks

3. **CPU profiling:**
   ```bash
   # Run benchmark in background
   cd tests/perf
   ./run_benchmark.sh &
   BENCH_PID=$!

   # Get rover PID
   sleep 2  # Let server start
   ROVER_PID=$(ps aux | grep 'rover' | grep -v grep | awk '{print $2}' | head -1)

   # Profile for 30 seconds
   perf record -p $ROVER_PID -g -F 99 sleep 30

   # Analyze
   perf report
   ```

4. **Look for hot functions:** Functions with >5% of CPU time are optimization candidates

5. **Check specific areas:**
   - `http_task.rs` - Lua handler execution
   - `to_json.rs` - JSON serialization
   - `lib.rs::handler` - HTTP parsing
   - `fast_router.rs` - Route matching
   - `event_loop.rs` - Task scheduling

### Phase 3: Optimize

6. **Make targeted changes** based on profiling results

7. **Common optimizations:**
   - **Reduce allocations:** Use `SmallVec`, stack buffers, `Arc` sharing
   - **Pre-compute:** Cache static routes, pre-serialize responses
   - **Batch operations:** Reduce lock/channel contention
   - **Lazy evaluation:** Don't parse headers/query unless used
   - **Zero-copy:** Use `Bytes` instead of `String` for responses

### Phase 4: Verify Improvement

8. **Re-run benchmark:**
   ```bash
   ./track_metrics.sh
   ```

9. **Compare results:** The script shows RPS change and P99 latency difference

10. **Accept or revert:**
    - If RPS increased or P99 decreased: ✅ Keep change
    - If performance regressed: ❌ Investigate or revert

## Key Metrics to Watch

### Primary Metrics
- **requests_per_sec**: Higher is better (throughput)
- **p99**: Lower is better (tail latency - 99% of users experience this or better)
- **success_rate**: Must be 100%

### Secondary Metrics
- **p50**: Median latency
- **p99.9**: Extreme tail latency (1 in 1000 requests)
- **mean**: Average latency (can be misleading, use p99 instead)
- **throughput_mb_per_sec**: Network throughput

### Warning Signs
- **p99 >> mean**: Occasional slowness (GC pauses, context switches)
- **max latency > 500ms**: Resource exhaustion, timeouts
- **errors > 0**: Server overload or bugs
- **memory growth**: Potential leak

## Advanced Profiling

### Flame Graph Visualization
```bash
# While benchmark runs
perf record -p <ROVER_PID> -g -F 99 sleep 30
perf script > out.perf

# Generate flame graph (requires flamegraph tools)
# Option 1: inferno (Rust)
cat out.perf | inferno-collapse-perf | inferno-flamegraph > flame.svg

# Option 2: FlameGraph (Perl)
stackcollapse-perf.pl out.perf | flamegraph.pl > flame.svg

# Open in browser
firefox flame.svg
```

### Memory Profiling
```bash
# Monitor memory during benchmark
# Terminal 1: Run benchmark
./run_benchmark.sh

# Terminal 2: Watch memory
watch -n 1 'ps aux | grep rover | head -1'

# Check for:
# - RSS growth (memory leak)
# - High RSS (excessive allocation)
```

### System-Level Monitoring
```bash
# CPU usage by core
mpstat -P ALL 1

# Context switches (high = contention)
pidstat -w -p <ROVER_PID> 1

# I/O wait (should be minimal for in-memory server)
iostat -x 1
```

## Benchmarking Best Practices

1. **Use release builds:** Always benchmark with `cargo build --release`
2. **Close other apps:** Minimize background noise
3. **Run multiple times:** Average 3-5 runs for stable results
4. **Warm up:** First run may be slower (JIT, caches)
5. **Consistent load:** Use same wrk settings for comparisons
6. **Document changes:** Track what changed between runs

## Tuning Benchmark Parameters

Edit `tests/perf/test.sh` to change load profile:

```bash
# Light load (find baseline latency)
wrk -t1 -c10 -d30s http://localhost:3000/echo

# Medium load (current default)
wrk -t2 -c100 -d30s http://localhost:3000/echo

# Heavy load (find breaking point)
wrk -t4 -c500 -d30s http://localhost:3000/echo

# Extreme load (stress test)
wrk -t8 -c1000 -d30s http://localhost:3000/echo
```

## Known Optimization Targets

Based on code review, these areas are likely bottlenecks:

| Area | File | Lines | Why |
|------|------|-------|-----|
| Lua context building | `http_task.rs` | 89-130 | Called per-request, many closures |
| JSON serialization | `to_json.rs` | 1-214 | Recursive, type checking |
| Body collection | `lib.rs` | 246-252 | Async stream reading |
| Route matching | `fast_router.rs` | 61-91 | Pattern matching, HashMap creation |
| Channel send/recv | `event_loop.rs` | 74-87 | Inter-task communication |

## Example Session

```bash
# 1. Baseline
$ cd tests/perf
$ ./track_metrics.sh
requests_per_sec=21958.15
p99=5.22ms

# 2. Profile
$ ./run_benchmark.sh &
$ perf record -p $(pgrep rover) -g -F 99 sleep 30
$ perf report
# Results show to_json::lua_to_json takes 35% of CPU time

# 3. Optimize JSON serialization
$ cd ../../rover_server/src
$ vim to_json.rs  # Make improvements

# 4. Rebuild and test
$ cd ../..
$ cargo build --release
$ cd tests/perf
$ ./track_metrics.sh
requests_per_sec=25234.67 (+15.0%)
p99=4.12ms (-21.1%)

# ✅ Improvement confirmed!
```

## Troubleshooting

### wrk not found
```bash
# Ubuntu/Debian
sudo apt-get install wrk

# macOS
brew install wrk

# Build from source
git clone https://github.com/wg/wrk.git
cd wrk && make
```

### perf not found
```bash
# Ubuntu/Debian
sudo apt-get install linux-tools-common linux-tools-generic

# Or use system perf
sudo apt-get install linux-tools-$(uname -r)
```

### Port already in use
```bash
# Kill existing rover processes
pkill -9 rover

# Or use different port in main.lua
vim tests/perf/main.lua
# Change port to 3001, 3002, etc.
```

### Permission denied for perf
```bash
# Temporary: Allow user perf
echo 0 | sudo tee /proc/sys/kernel/perf_event_paranoid

# Or run with sudo
sudo perf record -p <PID> -g -F 99 sleep 30
sudo perf report
```

## Next Steps

1. ✅ Run baseline benchmark: `./track_metrics.sh`
2. ✅ Profile to find bottleneck: `perf record + perf report`
3. ✅ Optimize hot function
4. ✅ Re-benchmark and verify improvement
5. ✅ Repeat until satisfied with performance

## References

- [wrk Documentation](https://github.com/wg/wrk)
- [Linux perf Tutorial](https://perf.wiki.kernel.org/)
- [Flame Graphs](https://www.brendangregg.com/flamegraphs.html)
- [Rust Performance Book](https://nnethercote.github.io/perf-book/)
