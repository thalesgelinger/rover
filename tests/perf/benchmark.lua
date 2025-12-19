wrk.method                  = "GET"
wrk.path                    = "/yabadabadoo"
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

    io.write("----------------------------------\n")
    io.write("Performance Summary\n")
    io.write("----------------------------------\n")

    -- Request stats
    io.write(string.format("Requests/sec:   %d\n", summary.requests / (summary.duration / 1000000)))
    io.write(string.format("Total requests: %d\n", summary.requests))
    io.write(string.format("Errors:         %d\n",
        summary.errors.connect + summary.errors.read + summary.errors.write + summary.errors.status +
        summary.errors.timeout))
    io.write(string.format("  Connect:      %d\n", summary.errors.connect))
    io.write(string.format("  Read:         %d\n", summary.errors.read))
    io.write(string.format("  Write:        %d\n", summary.errors.write))
    io.write(string.format("  Timeout:      %d\n", summary.errors.timeout))

    io.write("\n")

    -- Latency stats
    io.write("Latency Distribution\n")
    io.write(string.format("  Min:     %.2f ms\n", latency.min / 1000))
    io.write(string.format("  Mean:    %.2f ms\n", latency.mean / 1000))
    io.write(string.format("  Max:     %.2f ms\n", latency.max / 1000))
    io.write(string.format("  Stdev:   %.2f ms\n", latency.stdev / 1000))
    io.write(string.format("  p50:     %.2f ms\n", percentile(latency, 50.0) / 1000))
    io.write(string.format("  p75:     %.2f ms\n", percentile(latency, 75.0) / 1000))
    io.write(string.format("  p90:     %.2f ms\n", percentile(latency, 90.0) / 1000))
    io.write(string.format("  p95:     %.2f ms\n", percentile(latency, 95.0) / 1000))
    io.write(string.format("  p99:     %.2f ms\n", percentile(latency, 99.0) / 1000))
    io.write(string.format("  p99.9:   %.2f ms\n", percentile(latency, 99.9) / 1000))
    io.write(string.format("  p99.99:  %.2f ms\n", percentile(latency, 99.99) / 1000))

    io.write("\n")

    -- Throughput stats
    io.write("Throughput\n")
    io.write(string.format("  Bytes/sec:  %.2f MB\n", (summary.bytes / (summary.duration / 1000000)) / 1024 / 1024))

    io.write("\n")

    -- Status codes
    io.write("Status Codes\n")
    for code, count in pairs(status_codes) do
        io.write(string.format("  %d: %d (%.2f%%)\n", code, count, (count / summary.requests) * 100))
    end

    io.write("----------------------------------\n")
end
