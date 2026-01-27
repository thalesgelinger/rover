local ru = rover.ui

function rover.render()
	local value = rover.signal(0)

	-- Create a task that updates the value
	local tick = rover.task(function()
		while true do
			rover.delay(1000)
			value.val = value.val + 1
		end
	end)

	-- Start the task
	tick()

	rover.on_destroy(function()
		rover.task.cancel(tick)
	end)

	return ru.text { value }
end
