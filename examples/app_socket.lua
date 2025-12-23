local app = rover.app()
local ws = rover.ws_client("ws://")

function app.init()
    return ""
end

function ws.on.message(act, msg)
    act.new_message(msg)
end

function app.new_message(state)
    return state
end

function app.render(state)
    return rover.col {
        width = "full",
        height = 100,
        rover.text { "Message: " .. state },
    }
end
