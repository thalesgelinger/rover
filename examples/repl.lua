local ru = rover.ui

function rover.render()
	-- Log entries (reactive list of strings)
	local log = rover.signal({})
	local log_count = rover.signal(0)

	-- Uptime counter (background task)
	local uptime = rover.signal(0)
	local clock = rover.task(function()
		while true do
			rover.delay(1000)
			uptime.val = uptime.val + 1
		end
	end)
	clock()

	-- Status rotates through states
	local status = rover.signal("ready")
	local status_task = rover.task(function()
		local states = { "ready", "processing", "idle", "ready" }
		local i = 1
		while true do
			rover.delay(4000)
			i = (i % #states) + 1
			status.val = states[i]
		end
	end)
	status_task()

	rover.on_destroy(function()
		rover.task.cancel(clock)
		rover.task.cancel(status_task)
	end)

	-- Render the log entries as text nodes
	local log_list = ru.each(log, function(entry)
		return ru.text { entry }
	end, function(entry)
		return entry
	end)

	return ru.column {
		ru.text { "=== Rover REPL ===" },
		ru.row {
			ru.text { "uptime: " },
			ru.text { uptime },
			ru.text { "s | status: " },
			ru.text { status },
		},
		ru.text { "---" },
		log_list,
		ru.text { "---" },
		ru.row {
			ru.text { "> " },
			ru.input {
				on_submit = function(val)
					local entries = log.val
					entries[#entries + 1] = "> " .. val
					log.val = entries
					log_count.val = log_count.val + 1
				end,
			},
		},
		ru.row {
			ru.text { "entries: " },
			ru.text { log_count },
			ru.text { " | Ctrl+C to exit" },
		},
	}
end
