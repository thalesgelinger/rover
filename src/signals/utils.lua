local Utils = {}

function Utils.inspect(table)
    local function serialize(val, indent)
        local value = Utils.unwrap(val)
        if type(value) == "table" then
            local result = "{\n"
            for k, v in pairs(value) do
                result = result .. indent .. "  " .. tostring(k) .. " = " .. serialize(v, indent .. "  ") .. ",\n"
            end
            return result .. indent .. "}"
        else
            return tostring(val)
        end
    end

    local jsonString = serialize(table, "")

    for line in jsonString:gmatch("([^\n]+)") do
        io.write(line .. "\r\n")
    end
end

--- @param signal Signal | any
function Utils.unwrap(signal)
    if type(signal) == "table" and signal.get then
        return signal.get()
    else
        return signal
    end
end

return Utils
