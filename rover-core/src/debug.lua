-- Rover Debug Utilities
-- debug.print - Pretty-print values for debugging

local debug = debug or {}

debug.print = function(value, label)
    -- Format value with indentation
    local function format_val(v, depth, seen)
        depth = depth or 0
        seen = seen or {}

        if depth > 5 then
            return "<max_depth>"
        end

        local t = type(v)
        if t == "nil" then
            return "nil"
        elseif t == "boolean" then
            return tostring(v)
        elseif t == "number" then
            return tostring(v)
        elseif t == "string" then
            return '"' .. v:gsub('"', '\\"') .. '"'
        elseif t == "table" then
            -- Check for circular reference
            if seen[v] then
                return "<circular>"
            end
            seen[v] = true

            local indent = string.rep("  ", depth)
            local next_indent = string.rep("  ", depth + 1)
            local lines = { "{" }

            -- Check if array-like
            local is_array = true
            local max_idx = 0
            for k in pairs(v) do
                if type(k) == "number" then
                    if k > 0 then
                        max_idx = math.max(max_idx, k)
                    end
                else
                    is_array = false
                end
            end

            if is_array and max_idx > 0 then
                -- Array format
                for i = 1, max_idx do
                    if v[i] ~= nil then
                        table.insert(lines, next_indent .. format_val(v[i], depth + 1, seen) .. ",")
                    end
                end
            else
                -- Key-value format
                for k, val in pairs(v) do
                    local key_str = type(k) == "string" and k or tostring(k)
                    table.insert(lines, next_indent .. key_str .. " = " .. format_val(val, depth + 1, seen) .. ",")
                end
            end

            table.insert(lines, indent .. "}")
            return table.concat(lines, "\n")
        else
            return "<" .. t .. ">"
        end
    end

    local formatted = format_val(value)
    local output
    if label then
        output = string.format("[debug.print] %s: %s", label, formatted)
    else
        output = string.format("[debug.print] %s", formatted)
    end

    print(output)
    return value
end

return debug
