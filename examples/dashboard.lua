local ru = rover.ui

function rover.render()
	local seconds = rover.signal(0)
	local clicks = rover.signal(0)
	local status = rover.signal("running")

	-- Fast ticker: updates every second
	local clock = rover.task(function()
		while true do
			rover.delay(1000)
			seconds.val = seconds.val + 1
		end
	end)

	-- Slower ticker: simulates "clicks" every 3 seconds
	local clicker = rover.task(function()
		while true do
			rover.delay(3000)
			clicks.val = clicks.val + 1
		end
	end)

	-- Status changes after 5 seconds
	local status_task = rover.task(function()
		rover.delay(5000)
		status.val = "warmed up"
		rover.delay(5000)
		status.val = "on fire"
	end)

	clock()
	clicker()
	status_task()

	rover.on_destroy(function()
		rover.task.cancel(clock)
		rover.task.cancel(clicker)
		rover.task.cancel(status_task)
	end)

	return ru.column {
		ru.text { "=== Rover Dashboard ===" },
		ru.row {
			ru.text { "Uptime: " },
			ru.text { seconds },
			ru.text { "s" },
		},
		ru.row {
			ru.text { "Events: " },
			ru.text { clicks },
		},
		ru.row {
			ru.text { "Status: " },
			ru.text { status },
		},
		ru.text { "Press Ctrl+C to exit" },
	}
end
