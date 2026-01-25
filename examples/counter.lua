local ru = rover.ui

function rover.render()
	local value = rover.signal(0)

	local tick = rover.task(function()
		while true do
			value.val = value.val + 1
			rover.delay(1000)  -- No coroutine.yield() needed!
		end
	end)

	tick()

	return ru.text { value }
end
