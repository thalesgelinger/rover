require "rover.tui"
local ru = rover.ui
local mod = ru.mod

local messages = rover.signal {}

function rover.render()
	return ru.column {
        ru.view {
            mod = mod:height(1)
        },
		Header(),
		ru.text {
			"Welcome to open claudio",
			mod = mod:padding(1):color "#A8A8A8",
		},
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
			end,
		},
	}
end

function ChatInput(props)
	local value = rover.signal ""
	return ru.column {
		Border(),
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
		Border(),
	}
end

function Border()
	local border = ""

	for i = 1, ru.screen.width.val do
		border = border .. "─"
	end

	return ru.text { border }
end

function Header()
	return ru.row {
		Bot(),
		ru.column {
			ru.row {
				ru.text {
					"Open claudio code ",
				},
				ru.text {
					"v4.2.69",
					mod = mod:color "#A8A8A8",
				},
			},
			ru.text {
				"Modelo pica das galaxia",
				mod = mod:color "#A8A8A8",
			},
			ru.text {
				"~/Tamo/ae",
				mod = mod:color "#A8A8A8",
			},
		},
	}
end

function Bot()
	return ru.text {
		[[
  █▀▀█ 
  █  █ 
  ▀▀▀▀ 
        ]],
		mod = mod:height(3):color "#eaeaea",
	}
end
