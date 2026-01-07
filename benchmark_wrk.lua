wrk.method = "GET"
wrk.body   = nil
wrk.headers = {}

function request()
    if math.random(2) == 1 then
        path = "/optimized"
    else
        path = "/traditional"
    end
    return wrk.format(nil, path, headers, nil)
end

function done(summary, latency, requests)
    print("\nBenchmark Complete")
    print("================")
    print("Requests: " .. summary.requests)
    print("Requests/sec: " .. (summary.requests / (summary.duration / 1000000)))
    print("Latency (ms):")
    print("  Mean: " .. (latency.mean / 1000))
    print("  P50: " .. (latency:percentile(50) / 1000))
    print("  P95: " .. (latency:percentile(95) / 1000))
    print("  P99: " .. (latency:percentile(99) / 1000))
end
