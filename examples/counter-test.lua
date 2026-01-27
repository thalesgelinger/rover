-- Simple counter that increments a few times and exits
local ru = rover.ui
local count = rover.signal(0)

function rover.render()
	-- Create a task that updates the value 3 times
	local tick = rover.task(function()
		rover.delay(100)
		count.val = count.val + 1

		rover.delay(100)
		count.val = count.val + 1

		rover.delay(100)
		count.val = count.val + 1
	end)

	-- Start the task
	tick()

	return ru.text { count }
end
