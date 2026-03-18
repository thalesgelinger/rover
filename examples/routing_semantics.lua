local api = rover.server {}

function api.echo.get(ctx)
  return api.json {
    method = "GET",
    ok = true,
  }
end

function api.echo.post(ctx)
  return api.json:status(201, {
    method = "POST",
    ok = true,
  })
end

function api.users.p_id.get(ctx)
  return api.json {
    id = ctx:params().id,
  }
end

return api
