local api = rover.server {
	port = 3000,
	log_level = "nope",
}

-- GET endpoint - simple response
function api.echo.get(ctx)
	return api.json {
		message = "Echo GET",
		method = ctx.method,
	}
end

-- POST endpoint - echo back the request body
function api.echo.post(ctx)
	return api.json {
		message = "Echo POST",
		received_body = ctx:body() or "no body",
	}
end

return api
