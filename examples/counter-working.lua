-- Counter with signal and task in global scope
local ru = rover.ui

-- Create signal at module level so it persists across renders
_G.counter_value = rover.signal(0)

-- Create task at module level
_G.counter_task = rover.task(function()
	while true do
		rover.delay(1000)
		_G.counter_value.val = _G.counter_value.val + 1
	end
end)

function rover.render()
	-- Start the task
	_G.counter_task()

	rover.on_destroy(function()
		rover.task.cancel(_G.counter_task)
	end)

	return ru.text { _G.counter_value }
end
