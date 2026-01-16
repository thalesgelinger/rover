local server = rover.server {}
function server.hello.get(ctx)
	local params = ctx:params()
	return params
end
