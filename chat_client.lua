local ru = rover.ui
local function resolve_user_id()
	if arg ~= nil then
		for i = 1, 8 do
			local v = arg[i]
			if type(v) == "string" and v ~= "" and not v:match("%.lua$") then
				return v:gsub("%s+", "_")
			end
		end
	end

	local addr = tostring({}):match("0x(%x+)")
	local salt = tonumber(addr or "0", 16) or 0
	math.randomseed(os.time() + (salt % 1000000))
	return "u" .. tostring(os.time()) .. "-" .. tostring(math.random(1000, 9999))
end

local user_id = resolve_user_id()
local ws = rover.ws_client("ws://localhost:4242/chat?user_id=" .. user_id)
local started = false

local messages = rover.signal {}

function ws.join(ctx)
	print("connected as", user_id)
end

function ws.error(err)
	print("ws error:", tostring(err.message or "unknown"))
end

function ws.listen.message(msg)
	print("recv from", tostring(msg.user_id or "unknown"))
	local list = messages.val
	list[#list + 1] = tostring(msg.user_id or "unknown") .. ": " .. tostring(msg.message or "")
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
				list[#list + 1] = "you: " .. new_message
				messages.val = list
				ws.send.new_message { message = new_message }
			end,
		},
		ru.text { "user_id: " .. user_id },
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
