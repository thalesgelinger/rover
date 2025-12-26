local api = rover.server {}
local g = rover.guard

-- Helper function to clean error messages (remove stack traces)
local function clean_error(err)
    local err_str = tostring(err)
    -- Remove "runtime error: " prefix
    err_str = err_str:gsub("^runtime error: ", "")
    -- Remove stack traceback
    local stack_pos = err_str:find("\nstack traceback:")
    if stack_pos then
        err_str = err_str:sub(1, stack_pos - 1)
    end
    return err_str
end

-- Test 1: Clean error handling with custom messages
function api.users.post(ctx)
    local success, result = pcall(function()
        return ctx:body():expect {
            name = g:string():required("User name is required"),
            email = g:string():required("Email address must be provided"),
            age = g:integer():required("Age is required")
        }
    end)

    if not success then
        return api:error(400, clean_error(result))
    end

    return api.json:status(201, {
        message = "User created successfully",
        user = result
    })
end

-- Test 2: Product validation with enum
function api.products.post(ctx)
    local success, result = pcall(function()
        return ctx:body():expect {
            name = g:string():required("Product name is required"),
            category = g:string():enum({"electronics", "clothing", "food"}):required("Category must be one of: electronics, clothing, food"),
            price = g:number():required("Price is required"),
            stock = g:integer():default(0),
            active = g:boolean():default(true)
        }
    end)

    if not success then
        return api:error(422, clean_error(result))
    end

    return api.json:status(201, {
        message = "Product created",
        product = result
    })
end

-- Test 3: Default validation messages (when no custom message provided)
function api.simple.post(ctx)
    local success, result = pcall(function()
        return ctx:body():expect {
            title = g:string():required(),
            priority = g:integer():required(),
            done = g:boolean():default(false)
        }
    end)

    if not success then
        return api:error(400, clean_error(result))
    end

    return api.json(result)
end

return api
