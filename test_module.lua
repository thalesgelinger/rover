-- Test module for require() type checking
-- This module exports functions and variables

local version = "1.0.0"

-- Exported function with string return
local function greet(name)
	return "Hello, " .. name .. "!"
end

-- Exported function with number return
local function add(a, b)
	return a + b
end

-- Exported function with table return
local function get_config()
	return {
		enabled = true,
		timeout = 30,
		retries = 3,
	}
end

-- Return module table
return {
	greet = greet,
	add = add,
	get_config = get_config,
	version = version,
}
