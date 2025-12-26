local api = rover.server {}
local g = rover.guard
local ErrorHandler = require("error_handler")

-- Example 1: Simple validation with automatic error handling
function api.users.post(ctx)
    local success, result = pcall(function()
        return ctx:body():expect {
            name = g:string():required("Name is required"),
            email = g:string():required("Email is required"),
            age = g:integer()
        }
    end)

    -- Automatic handling: app errors return clean, runtime errors crash
    if not success then
        return ErrorHandler.handle(success, result, api, 400)
    end

    return api.json:status(201, {
        message = "User created",
        user = result
    })
end

-- Example 2: Business logic with custom app errors
function api.products.post(ctx)
    local success, result = pcall(function()
        local data = ctx:body():expect {
            name = g:string():required("Product name is required"),
            price = g:number():required("Price is required"),
            stock = g:integer():default(0)
        }

        -- Business logic validation
        if data.price < 0 then
            error("Price cannot be negative")  -- This will crash (runtime error)
        end

        return data
    end)

    if not success then
        return ErrorHandler.handle(success, result, api, 422)
    end

    return api.json:status(201, result)
end

-- Example 3: Demonstrating the difference
function api.demo.errors.get(ctx)
    local error_type = ctx:query().type

    if error_type == "validation" then
        -- Application error - clean response
        local success, result = pcall(function()
            return ctx:body():expect {
                required_field = g:string():required("This field is required!")
            }
        end)
        if not success then
            return ErrorHandler.handle(success, result, api, 400)
        end
    elseif error_type == "runtime" then
        -- Runtime error - will crash (developer bug)
        local x = nil
        return api.json({ value = x.nonexistent })  -- This will crash!
    end

    return api.json({ message = "Try ?type=validation or ?type=runtime" })
end

-- Example 4: Without ErrorHandler (manual approach)
function api.manual.post(ctx)
    local success, result = pcall(function()
        return ctx:body():expect {
            title = g:string():required("Title is required")
        }
    end)

    if not success then
        -- Manual check: is it ValidationErrors?
        if type(result) == "userdata" then
            -- Application error - return clean response
            return api:error(400, result)
        else
            -- Runtime error - crash so we can fix it
            error(result)
        end
    end

    return api.json(result)
end

return api
