local api = rover.server {}
local g = rover.guard

-- Test 1: Required field with custom error message
function api.users.post(ctx)
    local success, result = pcall(function()
        return ctx:body():expect {
            name = g:string():required("User name is required"),
            email = g:string():required("Email address is required"),
            age = g:integer():required("Age must be provided")
        }
    end)

    if not success then
        -- api:error now automatically handles ValidationErrors!
        return api:error(400, result)
    end

    return api.json:status(201, {
        message = "User created successfully",
        user = result
    })
end

-- Test 2: Multiple validation rules with custom messages
function api.products.post(ctx)
    local success, result = pcall(function()
        return ctx:body():expect {
            name = g:string():required("Product name is required"),
            category = g:string():enum({"electronics", "clothing", "food"}):required("Category must be one of: electronics, clothing, food"),
            price = g:number():required("Price is required and must be a number"),
            stock = g:integer():default(0)
        }
    end)

    if not success then
        return api:error(422, result)
    end

    return api.json:status(201, result)
end

-- Test 3: Nested object validation with custom messages
function api.orders.post(ctx)
    local success, result = pcall(function()
        return ctx:body():expect {
            customer = g:object {
                name = g:string():required("Customer name is required"),
                email = g:string():required("Customer email is required"),
                phone = g:string()
            },
            items = g:array(g:object {
                productId = g:integer():required("Product ID is required"),
                quantity = g:integer():required("Quantity is required")
            }):required("At least one item is required"),
            total = g:number():required("Total amount is required")
        }
    end)

    if not success then
        return api:error(400, result)
    end

    return api.json:status(201, {
        orderId = 12345,
        status = "confirmed"
    })
end

-- Test 4: Simple validation without custom messages (default errors)
function api.simple.post(ctx)
    local success, result = pcall(function()
        return ctx:body():expect {
            title = g:string():required(),
            count = g:integer():required()
        }
    end)

    if not success then
        return api:error(400, result)
    end

    return api.json(result)
end

return api
