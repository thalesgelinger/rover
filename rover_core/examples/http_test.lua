-- Simple HTTP client test - makes requests to itself

local api = rover.server {}

-- Simple endpoint that returns data
function api.data.get(ctx)
    return api.json {
        message = "Hello from data endpoint",
        timestamp = os.time(),
        value = 42
    }
end

-- Test GET request to localhost
function api.test_get.get(ctx)
    local response = rover.http.get("http://127.0.0.1:4242/data")

    return api.json {
        test = "GET to localhost",
        status = response.status,
        ok = response.ok,
        received_data = response.data
    }
end

-- Test POST request
function api.test_post.post(ctx)
    local body = ctx:body()

    return api.json {
        test = "POST received",
        received = body
    }
end

-- Test baseURL configuration
function api.test_client.get(ctx)
    local client = rover.http.create({
        baseURL = "http://127.0.0.1:4242",
        timeout = 5000
    })

    local response = client:get("/data")

    return api.json {
        test = "Custom client with baseURL",
        status = response.status,
        data = response.data
    }
end

-- Root endpoint
function api.get(ctx)
    return api.json {
        message = "HTTP Client Test Server",
        endpoints = {
            "GET /data - Returns test data",
            "GET /test_get - Tests GET request to /data",
            "POST /test_post - Tests POST request",
            "GET /test_client - Tests custom client with baseURL"
        }
    }
end

return api
