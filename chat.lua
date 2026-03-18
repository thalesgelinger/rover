local server = rover.server {}

function server.chat.ws(ws)
	function ws.join(ctx)
		local query = ctx:query()
		local user_id = query and query.user_id or nil
		if user_id == nil or user_id == "" then
			ws:error(4001, "user_id required")
			return
		end

		ws.send.user_joined():except {
			user_id = user_id,
		}

		return {
			user_id = user_id,
		}
	end

	function ws.listen.new_message(msg, ctx, state)
		if state == nil or state.user_id == nil then
			return
		end

		local text = tostring(msg.message or "")
		if text == "" then
			return
		end

		ws.send.message():except {
			user_id = state.user_id,
			message = text,
		}
	end

	function ws.leave(state)
		ws.send.user_left():except {
			user_id = state and state.user_id or "unknown",
		}
	end
end

return server
