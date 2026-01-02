local api = rover.server {}

-- Root endpoint
function api.get(ctx)
	return api.json {
		message = "Body Methods Examples",
		endpoints = {
			"POST /json - Parse JSON body using :json()",
			"POST /text - Get text body using :text()",
			"POST /bytes - Get bytes using :bytes()",
			"POST /raw - Parse raw JSON using :raw()",
			"POST /validated - Validate body using :expect()",
		},
	}
end

-- Example 1: Using :json() to parse JSON body
function api.json.post(ctx)
	local body = ctx:body():json()

	return api.json {
		method = ":json()",
		received = body,
		name = body.name or "anonymous",
		processed_at = os.date "%Y-%m-%d %H:%M:%S",
	}
end

-- Example 2: Using :text() to get raw text
function api.text.post(ctx)
	local text = ctx:body():text()

	return api.json {
		method = ":text()",
		text = text,
		length = #text,
		line_count = select(2, text:gsub("\n", "\n")) + 1,
		word_count = select(2, text:gsub("%S+", "")),
	}
end

-- Example 3: Using :bytes() to get raw bytes
function api.bytes.post(ctx)
	local bytes = ctx:body():bytes()

	local stats = {
		byte_count = #bytes,
	}

	if #bytes > 0 then
		stats.first_byte = bytes[1]
		stats.last_byte = bytes[#bytes]

		local sum = 0
		for i = 1, #bytes do
			sum = sum + bytes[i]
		end
		stats.byte_sum = sum
		stats.average = sum / #bytes
	end

	return api.json {
		method = ":bytes()",
		stats = stats,
	}
end

-- Example 4: Using :raw() to parse JSON (alias for :json())
function api.raw.post(ctx)
	local body = ctx:body():raw()

	return api.json {
		method = ":raw()",
		data = body,
		keys = {},
	}
end

-- Example 5: Using :expect() with validation
function api.validated.post(ctx)
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
		active = {
			type = "boolean",
			required = false,
			default = true,
		},
		tags = {
			type = "array",
			element = {
				type = "string",
			},
			required = false,
		},
		settings = {
			type = "object",
			schema = {
				notifications = {
					type = "boolean",
					required = false,
					default = true,
				},
				theme = {
					type = "string",
					required = false,
					default = "light",
					enum = { "light", "dark", "auto" },
				},
			},
			required = false,
		},
	}

	user.id = math.floor(math.random() * 10000)
	user.createdAt = os.date "%Y-%m-%dT%H:%M:%SZ"

	return api.json:status(201, {
		message = "User created successfully",
		user = user,
	})
end

-- Example 6: File upload simulation (bytes)
function api.upload.post(ctx)
	local bytes = ctx:body():bytes()

	return api.json {
		method = ":bytes()",
		message = "File uploaded",
		size = #bytes,
		mime_type = "application/octet-stream",
		uploaded_at = os.time(),
	}
end

-- Example 7: Echo endpoint that detects content type
function api.echo.post(ctx)
	local content_type = ctx:headers()["content-type"] or ""

	local result = {
		method = "echo",
		content_type = content_type,
	}

	if content_type:find "application/json" then
		result.data = ctx:body():json()
		result.parser = ":json()"
	elseif content_type:find "text/plain" then
		result.data = ctx:body():text()
		result.parser = ":text()"
	else
		result.data = ctx:body():bytes()
		result.parser = ":bytes()"
	end

	return api.json(result)
end

return api
