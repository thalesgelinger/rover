local api = rover.server {
  cors_origin = "*",
  cors_methods = "GET, POST, OPTIONS",
  cors_headers = "Content-Type, Authorization",
  cors_credentials = false,
}

function api.users.get(ctx)
  return api.json {
    users = { "alice", "bob" },
  }
end

function api.users.post(ctx)
  local body = ctx:body():json()
  return api.json:status(201, {
    created = true,
    user = body,
  })
end

return api
