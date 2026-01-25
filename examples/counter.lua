local ru = rover.ui

rover.render(function()
	local value = rover.signal(0)

	local tick = rover.task(function()
		while true do
			value.val = value.val + 1
			coroutine.yield(rover.delay(1000))
		end
	end)

	tick()

	return ru.text { value }
end)
