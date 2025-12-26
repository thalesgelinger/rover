-- Example demonstrating rover.http axios-style API

local api = rover.server {}

-- Example 1: Simple GET request
function api.example1.get(ctx)
    -- Simple GET request to JSONPlaceholder API
    local response = rover.http.get("https://jsonplaceholder.typicode.com/posts/1")

    return api.json {
        example = "Simple GET",
        status = response.status,
        ok = response.ok,
        data = response.data
    }
end

-- Example 2: GET with query parameters
function api.example2.get(ctx)
    local response = rover.http.get("https://jsonplaceholder.typicode.com/posts", {
        params = {
            userId = 1,
            _limit = 5
        }
    })

    return api.json {
        example = "GET with params",
        status = response.status,
        count = #response.data,
        data = response.data
    }
end

-- Example 3: POST request with data
function api.example3.post(ctx)
    local response = rover.http.post("https://jsonplaceholder.typicode.com/posts", {
        title = "foo",
        body = "bar",
        userId = 1
    })

    return api.json {
        example = "POST with data",
        status = response.status,
        created = response.data
    }
end

-- Example 4: Using a custom client with baseURL
function api.example4.get(ctx)
    -- Create a custom HTTP client with baseURL
    local jsonPlaceholder = rover.http.create({
        baseURL = "https://jsonplaceholder.typicode.com",
        timeout = 5000, -- 5 seconds
        headers = {
            ["User-Agent"] = "Rover-HTTP/1.0"
        }
    })

    -- Now we can use relative URLs
    local users = jsonPlaceholder:get("/users/1")
    local posts = jsonPlaceholder:get("/posts", {
        params = { userId = 1, _limit = 3 }
    })

    return api.json {
        example = "Custom client with baseURL",
        user = users.data,
        posts = posts.data
    }
end

-- Example 5: PUT request
function api.example5.put(ctx)
    local response = rover.http.put("https://jsonplaceholder.typicode.com/posts/1", {
        id = 1,
        title = "updated title",
        body = "updated body",
        userId = 1
    })

    return api.json {
        example = "PUT request",
        status = response.status,
        updated = response.data
    }
end

-- Example 6: DELETE request
function api.example6.delete(ctx)
    local response = rover.http.delete("https://jsonplaceholder.typicode.com/posts/1")

    return api.json {
        example = "DELETE request",
        status = response.status,
        ok = response.ok
    }
end

-- Example 7: Custom headers
function api.example7.get(ctx)
    local response = rover.http.get("https://jsonplaceholder.typicode.com/posts/1", {
        headers = {
            ["X-Custom-Header"] = "my-value",
            ["Accept"] = "application/json"
        }
    })

    return api.json {
        example = "Custom headers",
        status = response.status,
        data = response.data
    }
end

-- Example 8: Error handling
function api.example8.get(ctx)
    local success, response = pcall(function()
        return rover.http.get("https://invalid-domain-that-does-not-exist-12345.com")
    end)

    if success then
        return api.json {
            example = "Error handling",
            status = response.status,
            data = response.data
        }
    else
        return api.json:status(500, {
            example = "Error handling",
            error = tostring(response)
        })
    end
end

-- Example 9: Multiple concurrent requests
function api.example9.get(ctx)
    -- These will execute concurrently thanks to async!
    local user = rover.http.get("https://jsonplaceholder.typicode.com/users/1")
    local posts = rover.http.get("https://jsonplaceholder.typicode.com/posts?userId=1&_limit=3")
    local comments = rover.http.get("https://jsonplaceholder.typicode.com/comments?postId=1&_limit=3")

    return api.json {
        example = "Concurrent requests",
        user = user.data,
        posts = posts.data,
        comments = comments.data
    }
end

-- Root endpoint with documentation
function api.get(ctx)
    return api.json {
        message = "Rover HTTP Client Examples",
        endpoints = {
            "/example1 - Simple GET request",
            "/example2 - GET with query parameters",
            "/example3 - POST request with data",
            "/example4 - Custom client with baseURL",
            "/example5 - PUT request",
            "/example6 - DELETE request",
            "/example7 - Custom headers",
            "/example8 - Error handling",
            "/example9 - Multiple concurrent requests"
        },
        docs = "Visit each endpoint to see the HTTP client in action!"
    }
end

return api
