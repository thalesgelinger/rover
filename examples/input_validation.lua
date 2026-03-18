local api = rover.server {}

function api.users.post(ctx)
  local user = ctx:body():expect {
    name = rover.guard:string():required(),
    email = rover.guard:string():required(),
  }

  return api.json:status(201, {
    ok = true,
    user = user,
  })
end

function api.search.get(ctx)
  local q = ctx:query()
  if not q.term then
    return api.json:status(400, { error = "term is required" })
  end

  return api.json {
    term = q.term,
    page = tonumber(q.page or "1"),
  }
end

return api
