local api = rover.server {}

-- Endpoint with authorization check
function api.hello.get(ctx)
	local token = ctx:headers().Authorization

	if not token then
		return api.json:status(401, {
			message = "Unauthorized",
		})
	end

	return api.json:status(200, {
		message = "Hello World",
	})
end

-- Nested route with GET
function api.hello.world.get(ctx)
	return api.json {
		message = "Hello World Nested",
	}
end

-- Nested route with POST
function api.hello.world.post(ctx)
	return api.json {
		message = "Post to Hello World",
	}
end

-- GET users list
function api.users.get(ctx)
	return api.json {
		users = {},
	}
end

-- POST to create user using :json() method
function api.users.create.post(ctx)
	local body = ctx:body():json()
	body.id = 1

	return api.json(body)
end

-- Echo text endpoint using :text() method
function api.raw_content.post(ctx)
	local text = ctx:body():text()
	return api.json {
		received = text,
		length = #text,
	}
end

-- Echo bytes endpoint using :bytes() method
function api.echo.bytes.post(ctx)
	local bytes = ctx:body():bytes()
	local sum = 0

	for i = 1, #bytes do
		sum = sum + bytes[i]
	end

	return api.json {
		byte_count = #bytes,
		sum = sum,
		first_byte = bytes[1] or 0,
		last_byte = bytes[#bytes] or 0,
	}
end

-- Validated user creation using :expect()
function api.users.validated.post(ctx)
	local user = ctx:body():expect {
		name = {
			type = "string",
			required = true,
		},
		email = {
			type = "string",
			required = true,
		},
		age = {
			type = "integer",
			required = false,
			default = 18,
		},
	}

	user.id = math.floor(math.random() * 1000)
	user.createdAt = os.date "%Y-%m-%d"

	return api.json:status(201, user)
end

return api
