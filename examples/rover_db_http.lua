local api = rover.server {}
local db = rover.db.connect()
local g = rover.guard

function api.users.get(ctx)
    local users = db.users:find():all()
    return api.json(users)
end

function api.users.post(ctx)
    local body = ctx:body():expect {
        name = g:string():required "Name is required",
        email = g:string():required "Email is required",
    }

    local result = db.users:insert({
        name = body.name,
        email = body.email,
        status = body.status or "active"
    })

    return api.json:status(201, result)
end

function api.users.p_id.get(ctx)
    local id = tonumber(ctx:params().id)
    local user = db.users:find():by_id(id):first()

    if not user then
        return api:error(404, "User not found")
    end

    return api.json(user)
end

function api.users.p_id.put(ctx)
    local id = tonumber(ctx:params().id)
    local body = ctx:body():json()

    local existing = db.users:find():by_id(id):first()

    if not existing then
        return api:error(404, "User not found")
    end

    db.users:update()
        :by_id(id)
        :set({
            name = body.name or existing.name,
            email = body.email or existing.email,
            status = body.status or existing.status
        })
        :exec()

    local updated = db.users:find():by_id(id):first()
    return api.json(updated)
end

function api.users.p_id.delete(ctx)
    local id = tonumber(ctx:params().id)

    local user = db.users:find():by_id(id):first()
    if not user then
        return api:error(404, "User not found")
    end

    db.users:delete():by_id(id):exec()

    return api.json({ message = "User deleted" })
end

function api.users.p_id.orders.get(ctx)
    local id = tonumber(ctx:params().id)

    local user = db.users:find():by_id(id):first()
    if not user then
        return api:error(404, "User not found")
    end

    local orders = db.orders:find():by_user_id(id):all()

    return api.json({
        user = user,
        orders = orders,
        count = #orders
    })
end

function api.users.p_id.orders.post(ctx)
    local id = tonumber(ctx:params().id)
    local body = ctx:body():json()

    local user = db.users:find():by_id(id):first()
    if not user then
        return api:error(404, "User not found")
    end

    if not body.amount or not body.status then
        return api:error(400, "amount and status required")
    end

    local order = db.orders:insert({
        user_id = id,
        amount = body.amount,
        status = body.status
    })

    return api.json:status(201, order)
end

function api.stats.get(ctx)
    local user_count = db.users:find():count()
    
    local order_stats = db.orders:find()
        :group_by(db.orders.user_id)
        :agg({
            total = rover.db.sum(db.orders.amount),
            count = rover.db.count(db.orders.id)
        })
        :all()

    return api.json({
        total_users = user_count,
        user_orders = order_stats
    })
end

return api
