-- Test that regular Lua I/O works at top level (outside handlers)

print("Testing top-level file I/O...")

-- Write a file at module load time
local file = io.open("/tmp/rover_top_level_test.txt", "w")
file:write("This was written at top level!\n")
file:write("Line 2 from top level\n")
file:close()

print("File written successfully")

-- Read it back
local file2 = io.open("/tmp/rover_top_level_test.txt", "r")
local content = file2:read("*a")
file2:close()

print("File content:", content)

-- Create the server
local api = rover.server {}

function api.test.get(ctx)
    -- Also test inside handler
    local f = io.open("/tmp/rover_handler_test.txt", "w")
    f:write("Written from handler\n")
    f:close()
    
    local f2 = io.open("/tmp/rover_handler_test.txt", "r")
    local handler_content = f2:read("*a")
    f2:close()
    
    return api.json {
        top_level_worked = content,
        handler_worked = handler_content
    }
end

return api
