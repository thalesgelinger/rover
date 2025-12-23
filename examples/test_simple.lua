local api = rover.server {}
local g = rover.guard

function api.test.get(ctx)
    -- Test rover.guard directly without body parsing
    local success, result = pcall(function()
        local data = {
            name = "John",
            age = 30
        }
        
        local name_val = g:string()
        name_val:required()
        
        local age_val = g:integer()
        
        return rover.guard(data, {
            name = name_val,
            age = age_val
        })
    end)
    
    if not success then
        return api:error(400, result)
    end
    
    return api.json(result)
end

return api
