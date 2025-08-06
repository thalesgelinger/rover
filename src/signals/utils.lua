local Utils = {}

function Utils.inspect(table)
    local function serialize(value, indent)
        if type(value) == "table" then
            local result = "{\n"
            for k, v in pairs(value) do
                result = result .. indent .. "  [" .. tostring(k) .. "] = " .. serialize(v, indent .. "  ") .. ",\n"
            end
            return result .. indent .. "}"
        else
            return tostring(value)
        end
    end

    return serialize(table, "")
end

return Utils
