local ru = rover.ui

function rover.render()
	local count = rover.signal(0)
	local double = count * 2

	local tick = rover.task(function()
		while true do
			rover.delay(1000)
			count.val = count.val + 1
		end
	end)

	tick()

	rover.on_destroy(function()
		rover.task.cancel(tick)
	end)

	return ru.column {
		ru.text { "Rover TUI Counter" },
		ru.row {
			ru.text { "Count: " .. count },
			ru.column {
				ru.text { double },
			},
		},
		ru.text { "Press Ctrl+C to exit" },
	}
end
