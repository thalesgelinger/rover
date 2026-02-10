require "rover.tui"

local ui = rover.ui
local mod = ui.mod

local tick = rover.signal(0)

rover.interval(400, function()
	tick.val = tick.val + 1
end)

function rover.render()
	local pulse = rover.derive(function()
		if tick.val % 2 == 0 then
			return "accent"
		end
		return "warning"
	end)

	return ui.full_screen {
		ui.view {
			mod = mod:width("full"):height("full"):bg_color("surface"):padding "md",
			ui.stack {
				mod = mod:width("full"):height "full",

				ui.view {
					mod = mod:width(40)
						:height(12)
						:horizontal("center")
						:vertical("center")
						:border_color("accent")
						:border_width(1)
						:padding("sm")
						:bg_color "surface_alt",
					ui.column {
						ui.text { "modifiers + theme" },
						ui.text { "tick: " .. tostring(tick.val) },
						ui.row {
							ui.view {
								mod = mod:border_color("info"):border_width(1):padding("sm"):bg_color "#0b3a53",
								ui.text { "bento a" },
							},
							ui.view {
								mod = mod:border_color("danger"):border_width(1):padding("sm"):bg_color "#4a1111",
								ui.text { "bento b" },
							},
						},
					},
				},

				ui.view {
					mod = mod:left(12)
						:top(2)
						:position("absolute")
						:border_color("info")
						:border_width(1)
						:padding("xs")
						:bg_color(pulse),
					ui.text { "badge" },
				},
			},
		},
	}
end
