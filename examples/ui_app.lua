local app = rover.app()

-- Initialize state with counter value
function app.init()
	return 0
end

-- Action to increase counter
function app.increase(state)
	return state + 1
end

-- Action to decrease counter
function app.decrease(state)
	return state - 1
end

-- Render UI based on state
function app.render(state)
	return rover.col {
		width = "full",
		height = 100,
		rover.text { "Count: " .. state },
		rover.row {
			rover.button { "Increase", press = "increase" },
			rover.button { "Decrease", press = "decrease" },
		},
	}
end
