local ru = rover.ui

function rover.render()
	local value = rover.signal(0)

	local tick = rover.task(function()
		while true do
			value.val = value.val + 1
			coroutine.yield(rover.delay(1000))
		end
	end)

	tick()

	return ru.text { value }
end
