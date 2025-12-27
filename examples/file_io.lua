-- Example demonstrating async file I/O using overridden io module
-- The io module now uses Tokio async operations under the hood!

local api = rover.server { }

-- Example 1: Write to file
function api.write.p_name.get(ctx)
    local file = io.open("/tmp/rover_test.txt", "w")
    file:write("Hello from Rover async I/O!\n")
    file:write("This is line 2\n")
    file:write("And line 3")
    file:close()

    return api.json {
        example = "File write",
        message = "Successfully wrote to /tmp/rover_test.txt"
    }
end

-- Example 2: Read entire file
function api.read_all.get(ctx)
    local file = io.open("/tmp/rover_test.txt", "r")
    local content = file:read("*a")
    file:close()

    return api.json {
        example = "Read entire file",
        content = content
    }
end

-- Example 3: Read line by line
function api.read_lines.get(ctx)
    local file = io.open("/tmp/rover_test.txt", "r")
    local lines = {}

    local line1 = file:read("*l")  -- Read line without newline
    local line2 = file:read("*l")
    local line3 = file:read("*l")

    file:close()

    return api.json {
        example = "Read lines",
        line1 = line1,
        line2 = line2,
        line3 = line3
    }
end

-- Example 4: Append to file
function api.append.get(ctx)
    local file = io.open("/tmp/rover_test.txt", "a")
    file:write("\nAppended line!")
    file:close()

    -- Read it back
    local file2 = io.open("/tmp/rover_test.txt", "r")
    local content = file2:read("*a")
    file2:close()

    return api.json {
        example = "Append to file",
        content = content
    }
end

-- Example 5: io.lines() - read all lines into table
function api.all_lines.get(ctx)
    local lines = io.lines("/tmp/rover_test.txt")

    return api.json {
        example = "io.lines()",
        line_count = #lines,
        lines = lines
    }
end

-- Example 6: File seek operations
function api.seek.get(ctx)
    local file = io.open("/tmp/rover_test.txt", "r")

    -- Read first 5 bytes
    local start = file:read(5)

    -- Get current position
    local pos = file:seek("cur", 0)

    -- Seek to position 10
    file:seek("set", 10)
    local at_10 = file:read(5)

    -- Seek to end
    local file_size = file:seek("end", 0)

    file:close()

    return api.json {
        example = "File seek",
        first_5_bytes = start,
        position_after = pos,
        at_position_10 = at_10,
        file_size = file_size
    }
end

-- Example 7: Read specific number of bytes
function api.read_bytes.get(ctx)
    local file = io.open("/tmp/rover_test.txt", "r")

    local chunk1 = file:read(10)  -- Read 10 bytes
    local chunk2 = file:read(10)  -- Read next 10 bytes

    file:close()

    return api.json {
        example = "Read N bytes",
        chunk1 = chunk1,
        chunk2 = chunk2
    }
end

-- Example 8: Write and read in same handler (demonstrating async!)
function api.concurrent.get(ctx)
    -- Write a file
    local write_file = io.open("/tmp/test1.txt", "w")
    write_file:write("Test file 1")
    write_file:close()

    -- Write another file
    local write_file2 = io.open("/tmp/test2.txt", "w")
    write_file2:write("Test file 2")
    write_file2:close()

    -- Read both files
    local read_file1 = io.open("/tmp/test1.txt", "r")
    local read_file2 = io.open("/tmp/test2.txt", "r")

    local content1 = read_file1:read("*a")
    local content2 = read_file2:read("*a")

    read_file1:close()
    read_file2:close()

    return api.json {
        example = "Concurrent file operations",
        file1 = content1,
        file2 = content2,
        note = "All operations are async and non-blocking!"
    }
end

-- Example 9: Error handling
function api.error_handling.get(ctx)
    local success, err = pcall(function()
        local file = io.open("/nonexistent/path/file.txt", "r")
        return file:read("*a")
    end)

    if success then
        return api.json { result = "success" }
    else
        return api.json:status(500, {
            example = "Error handling",
            error = tostring(err),
            message = "File operation failed as expected"
        })
    end
end

-- Root endpoint with documentation
function api.get(ctx)
    return api.json {
        message = "Async File I/O Examples",
        note = "All file operations use Tokio async I/O - fully non-blocking!",
        endpoints = {
            "/write - Write to /tmp/rover_test.txt",
            "/read_all - Read entire file",
            "/read_lines - Read line by line",
            "/append - Append to file",
            "/all_lines - Use io.lines() to read all lines",
            "/seek - Demonstrate file seek operations",
            "/read_bytes - Read specific number of bytes",
            "/concurrent - Multiple file operations",
            "/error_handling - Error handling example"
        },
        api_compatibility = "Uses standard Lua io API - just async under the hood!"
    }
end

return api
