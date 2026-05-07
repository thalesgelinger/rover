local port = tonumber(os.getenv "ROVER_E2E_PORT") or 4242
local cert_file = os.getenv "ROVER_E2E_CERT"
local key_file = os.getenv "ROVER_E2E_KEY"
local http2 = os.getenv "ROVER_E2E_HTTP2" == "1"

local api = rover.server {
	port = port,
	tls = {
		cert_file = cert_file,
		key_file = key_file,
	},
	http2 = http2,
}

function api.hello.get(ctx)
	return api.json {
		message = "https ok",
	}
end

function api.echo.post(ctx)
	return api.json {
		body = ctx:body():text(),
	}
end

function api.flow.chunks.get(ctx)
	local chunks = { "one", ":", "two" }
	local i = 0
	return api:stream(200, "text/plain", function()
		i = i + 1
		return chunks[i]
	end)
end

function api.events.get(ctx)
	local sent = false
	return api.sse(function()
		if sent then
			return nil
		end
		sent = true
		return {
			event = "ready",
			data = "h2 sse",
			id = "1",
		}
	end)
end

return api
