local api = rover.server {}
local g = rover.guard

-- Example 1: Basic body validation with required and optional fields
function api.users.post(ctx)
    local success, user = pcall(function()
        return ctx:body():expect {
            name = g:string():required("Missing name param"),
            email = g:string():required("Email is required"),
            age = g:integer(),
            bio = g:string()
        }
    end)

    if not success then
        return api:error(400, user)
    end

    print("User created:", user.name, user.email, user.age or "no age", user.bio or "no bio")
    return api.json:status(201, { id = 123, name = user.name })
end

-- Example 2: Enum validation
function api.products.post(ctx)
    local success, product = pcall(function()
        return ctx:body():expect {
            name = g:string():required(),
            category = g:string():enum({ "electronics", "clothing", "food" }):required(),
            price = g:number():required()
        }
    end)

    if not success then
        return api:error(400, product)
    end

    return api.json { product = product }
end

-- Example 3: Nested objects
function api.orders.post(ctx)
    local success, order = pcall(function()
        return ctx:body():expect {
            customer = g:object {
                name = g:string():required(),
                email = g:string():required()
            },
            total = g:number():required()
        }
    end)

    if not success then
        return api:error(400, order)
    end

    print("Order from:", order.customer.name, order.customer.email, "Total:", order.total)
    return api.json:status(201, { orderId = 456 })
end

-- Example 4: Arrays
function api.tags.post(ctx)
    local success, data = pcall(function()
        return ctx:body():expect {
            tags = g:array(g:string()):required(),
            counts = g:array(g:integer())
        }
    end)

    if not success then
        return api:error(400, data)
    end

    print("Tags:", #data.tags, "items")
    return api.json { received = data.tags }
end

-- Example 5: Array of objects
function api.items.post(ctx)
    local success, data = pcall(function()
        return ctx:body():expect {
            items = g:array(g:object {
                id = g:integer():required(),
                name = g:string():required(),
                active = g:boolean()
            }):required()
        }
    end)

    if not success then
        return api:error(400, data)
    end

    print("Received", #data.items, "items")
    return api.json:status(201, { count = #data.items })
end

-- Example 6: Default values
function api.config.post(ctx)
    local success, config = pcall(function()
        return ctx:body():expect {
            theme = g:string():default("light"),
            notifications = g:boolean():default(true),
            maxItems = g:integer():default(10)
        }
    end)

    if not success then
        return api:error(400, config)
    end

    print("Config:", config.theme, config.notifications, config.maxItems)
    return api.json { config = config }
end

-- Example 7: Using rover.guard as a callable function (generic validation)
function api.validate.post(ctx)
    local body_success, body_data = pcall(function()
        return ctx:body():expect {
            data = g:object {
                value = g:string():required()
            }
        }
    end)

    if not body_success then
        return api:error(400, body_data)
    end

    -- Now validate something else with rover.guard directly
    local validation_success, validated = pcall(function()
        local some_data = {
            status = "active",
            count = 5
        }

        return rover.guard(some_data, {
            status = g:string():enum({ "active", "inactive" }),
            count = g:integer():required()
        })
    end)

    if not validation_success then
        return api:error(500, validated)
    end

    return api.json {
        body = body_data,
        validated = validated
    }
end

-- Example 8: Complex nested structure
function api.complex.post(ctx)
    local success, data = pcall(function()
        return ctx:body():expect {
            user = g:object {
                name = g:string():required(),
                email = g:string():required(),
                address = g:object {
                    street = g:string(),
                    city = g:string():required(),
                    zipCode = g:string()
                }
            },
            items = g:array(g:object {
                productId = g:integer():required(),
                quantity = g:integer():required(),
                metadata = g:object {
                    color = g:string(),
                    size = g:string()
                }
            }),
            totalAmount = g:number():required(),
            currency = g:string():enum({ "USD", "EUR", "BRL" }):default("USD")
        }
    end)

    if not success then
        return api:error(400, data)
    end

    print("Complex order received")
    return api.json:status(201, { success = true })
end

return api
