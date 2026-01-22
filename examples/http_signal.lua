local server = rover.server {}

local value = rover.signal(0)

rover.effect(function()
	print("VALUE: ", value.val)
end)

function server.hey.get()
	value.val = value.val + 1
	return server.json {
		value = value.val,
	}
end

return server
