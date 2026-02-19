local ru = rover.ui
local ws = rover.ws_client "ws://localhost:4242/chat"
local started = false

local messages = rover.signal {}

function ws.join(ctx) end

function ws.error(err)
	print("ws error:", tostring(err.message or "unknown"))
end

function ws.listen.message(msg)
	print "Echoed"
	local list = messages.val
	list[#list + 1] = msg.message
	messages.val = list
end

function ws.leave(ctx) end

function rover.render()
	if not started then
		started = true
		ws:connect()

		rover.on_destroy(function()
			if ws:is_connected() then
				ws:close(1000, "shutdown")
			end
		end)
	end

	return ru.column {
		ru.each(messages, function(item, index)
			return ru.text { item }
		end, function(item, index)
			return tostring(index)
		end),
		ChatInput {
			on_new_message = function(new_message)
				local list = messages.val
				list[#list + 1] = new_message
				messages.val = list
				ws.send.new_message { message = new_message }
			end,
		},
	}
end

function ChatInput(props)
	local value = rover.signal ""
	return ru.column {
		ru.row {
			ru.text { "❯ " },
			ru.input {
				value = value,
				on_submit = function(val)
					local text = (val or ""):gsub("^%s+", ""):gsub("%s+$", "")
					if text == "" then
						return
					end
					props.on_new_message(text)
					value.val = ""
				end,
			},
		},
	}
end
