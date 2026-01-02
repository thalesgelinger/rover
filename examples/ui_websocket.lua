local app = rover.app()
local ws = rover.ws_client "ws://"

-- Initialize state for messages
function app.init()
	return ""
end

-- Handle incoming WebSocket messages
function ws.on.message(act, msg)
	act.emit:new_message(msg)
end

-- Action to update state with new message
function app.new_message(state, msg)
	return msg
end

-- Render UI with latest message
function app.render(state)
	return rover.col {
		width = "full",
		height = 100,
		rover.text { "Message: " .. state },
	}
end
