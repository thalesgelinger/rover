wrk.method                  = "GET"
wrk.path                    = "/echo"
wrk.headers["Content-Type"] = "application/json"

-- Track status codes
local status_codes          = {}

function response(status, headers, body)
    status_codes[status] = (status_codes[status] or 0) + 1
end

function done(summary, latency, requests)
    -- Calculate percentiles
    local function percentile(stat, p)
        return stat:percentile(p)
    end

    local rps = summary.requests / (summary.duration / 1000000)
    local total_errors = summary.errors.connect + summary.errors.read + summary.errors.write +
                         summary.errors.status + summary.errors.timeout
    local throughput_mb = (summary.bytes / (summary.duration / 1000000)) / 1024 / 1024
    local success_rate = ((summary.requests - total_errors) / summary.requests) * 100

    io.write("\n")
    io.write("=== BENCHMARK RESULTS ===\n")
    io.write("\n")

    -- Summary metrics (easy to parse)
    io.write("SUMMARY:\n")
    io.write(string.format("  total_requests=%d\n", summary.requests))
    io.write(string.format("  requests_per_sec=%.0f\n", rps))
    io.write(string.format("  total_errors=%d\n", total_errors))
    io.write(string.format("  success_rate=%.2f%%\n", success_rate))
    io.write(string.format("  duration_sec=%.1f\n", summary.duration / 1000000))
    io.write(string.format("  throughput_mb_per_sec=%.2f\n", throughput_mb))
    io.write("\n")

    -- Latency metrics
    io.write("LATENCY_MS:\n")
    io.write(string.format("  min=%.2f\n", latency.min / 1000))
    io.write(string.format("  mean=%.2f\n", latency.mean / 1000))
    io.write(string.format("  stdev=%.2f\n", latency.stdev / 1000))
    io.write(string.format("  max=%.2f\n", latency.max / 1000))
    io.write(string.format("  p50=%.2f\n", percentile(latency, 50.0) / 1000))
    io.write(string.format("  p75=%.2f\n", percentile(latency, 75.0) / 1000))
    io.write(string.format("  p90=%.2f\n", percentile(latency, 90.0) / 1000))
    io.write(string.format("  p95=%.2f\n", percentile(latency, 95.0) / 1000))
    io.write(string.format("  p99=%.2f\n", percentile(latency, 99.0) / 1000))
    io.write(string.format("  p99.9=%.2f\n", percentile(latency, 99.9) / 1000))
    io.write(string.format("  p99.99=%.2f\n", percentile(latency, 99.99) / 1000))
    io.write("\n")

    -- Error breakdown
    io.write("ERRORS:\n")
    io.write(string.format("  connect=%d\n", summary.errors.connect))
    io.write(string.format("  read=%d\n", summary.errors.read))
    io.write(string.format("  write=%d\n", summary.errors.write))
    io.write(string.format("  status=%d\n", summary.errors.status))
    io.write(string.format("  timeout=%d\n", summary.errors.timeout))
    io.write("\n")

    -- Status codes
    io.write("STATUS_CODES:\n")
    for code, count in pairs(status_codes) do
        local percentage = (count / summary.requests) * 100
        io.write(string.format("  %d=%d (%.1f%%)\n", code, count, percentage))
    end

    io.write("\n")
    io.write("=== END RESULTS ===\n")
    io.write("\n")
end
