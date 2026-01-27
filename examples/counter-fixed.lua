-- Counter with signal created outside render
local value = rover.signal(0)

local tick = rover.task(function()
	while true do
		value.val = value.val + 1
		rover.delay(1000)
	end
end)

function rover.render()
	tick()

	rover.on_destroy(function()
		rover.task.cancel(tick)
	end)

	return rover.ui.text { value }
end
