local Utils = {}

---@param child table<string, any>
function Utils.parse_children(child)
    local props = {}
    local children = {}

    for k, v in pairs(child) do
        if type(k) ~= "number" then
            props[k] = v
        else
            table.insert(children, v)
        end
    end

    return {
        props = props,
        children = children
    }
end

function Utils.show_all(child)
    for k, v in pairs(child) do
        if type(v) == "table" then
            print("ID: ", v["id"])
            Utils.show_all(v)
        else
            print(k, v)
        end
    end
end

return Utils
