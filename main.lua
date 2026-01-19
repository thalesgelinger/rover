local api = rover.server {}

local g = rover.guard

local users = {}

function api.users.get(ctx)
	return api.json(users)
end

function api.users.post(ctx)
	local user = ctx:body():json():expect {
        name = g:string()
    }

    table.insert(users, user)

	return api.json {
		message = "ok",
	}
end

return api
