local ui = rover.ui

function rover.render()
	local count = rover.signal(0)
	local name = rover.signal ""
	local enabled = rover.signal(true)

	local function swatch(label, color)
		return ui.row {
			style = { gap = 8, width = "full" },
			ui.view {
				style = {
					width = 44,
					height = 24,
					bg_color = color,
					border_color = "#0f172a",
					border_width = 1,
				},
			},
			ui.text {
				label,
				style = { color = color },
			},
		}
	end

	return ui.scroll_view {
		style = { width = "full", height = "full", bg_color = "#f8fafc" },
		ui.column {
			style = { padding = 24, gap = 12, width = "full", bg_color = "#f8fafc" },
			ui.text {
				"Rover iOS",
				style = { color = "#0f172a" },
			},
			ui.text {
				"Native UIKit renderer",
				style = { color = "#2563eb" },
			},
			ui.text {
				"Count: " .. count,
				style = { color = "#16a34a" },
			},

			ui.row {
				style = { gap = 8, width = "full" },
				ui.button {
					label = "-1",
					style = {
						padding = 8,
						bg_color = "#fee2e2",
						border_color = "#f87171",
						border_width = 1,
						color = "#b91c1c",
					},
					on_click = function()
						count.val = count.val - 1
					end,
				},
				ui.button {
					label = "Increment",
					style = {
						padding = 8,
						bg_color = "#dbeafe",
						border_color = "#60a5fa",
						border_width = 1,
						color = "#1d4ed8",
					},
					on_click = function()
						count.val = count.val + 1
					end,
				},
				ui.button {
					label = "Reset",
					style = {
						padding = 8,
						bg_color = "#fef3c7",
						border_color = "#f59e0b",
						border_width = 1,
						color = "#92400e",
					},
					on_click = function()
						count.val = 0
					end,
				},
			},

			ui.input {
				value = name,
				on_change = function(value)
					name.val = value
				end,
			},
			ui.text {
				"Hello " .. name,
				style = { color = "#9333ea" },
			},
			ui.column {
				style = {
					padding = 12,
					gap = 8,
					width = "full",
					bg_color = "#ecfeff",
					border_color = "#06b6d4",
					border_width = 1,
				},
				ui.text {
					"Styled native section",
					style = { color = "#0e7490" },
				},
				ui.text {
					"Background, border, text color",
					style = { color = "#475569" },
				},
				ui.row {
					style = { gap = 8, width = "full" },
					ui.column {
						style = {
							padding = 8,
							bg_color = "#dcfce7",
							border_color = "#22c55e",
							border_width = 1,
						},
						ui.text { "left column", style = { color = "#15803d" } },
						ui.text { "inside row", style = { color = "#166534" } },
					},
					ui.column {
						style = {
							padding = 8,
							bg_color = "#f3e8ff",
							border_color = "#a855f7",
							border_width = 1,
						},
						ui.text { "right column", style = { color = "#7e22ce" } },
						ui.text { "side by side", style = { color = "#6b21a8" } },
					},
				},
			},

			ui.column {
				style = {
					padding = 12,
					gap = 8,
					width = "full",
					bg_color = "#fff7ed",
					border_color = "#fb923c",
					border_width = 1,
				},
				ui.text { "Row swatches", style = { color = "#c2410c" } },
				swatch("blue row item", "#2563eb"),
				swatch("green row item", "#16a34a"),
				swatch("purple row item", "#9333ea"),
			},

			ui.checkbox {
				checked = enabled.val,
				on_toggle = function(value)
					enabled.val = value
				end,
			},
			ui.text { "Vertical scroll", style = { color = "#0f172a" } },
			ui.scroll_view {
				style = {
					height = 180,
					width = "full",
					bg_color = "#f1f5f9",
					border_color = "#94a3b8",
					border_width = 1,
				},
				ui.column {
					style = { gap = 8 },
					ui.text { "UIKit views", style = { color = "#dc2626" } },
					ui.text { "Rover signals drive updates", style = { color = "#ea580c" } },
					ui.text { "Typed native bridge, no JSON", style = { color = "#0891b2" } },
					ui.row {
						style = { gap = 8 },
						ui.text { "scroll row 1", style = { color = "#475569" } },
						ui.text { "blue", style = { color = "#2563eb" } },
					},
					ui.row {
						style = { gap = 8 },
						ui.text { "scroll row 2", style = { color = "#475569" } },
						ui.text { "green", style = { color = "#16a34a" } },
					},
					ui.row {
						style = { gap = 8 },
						ui.text { "scroll row 3", style = { color = "#475569" } },
						ui.text { "purple", style = { color = "#9333ea" } },
					},
					ui.row {
						style = { gap = 8 },
						ui.text { "scroll row 4", style = { color = "#475569" } },
						ui.text { "orange", style = { color = "#ea580c" } },
					},
					ui.row {
						style = { gap = 8 },
						ui.text { "scroll row 5", style = { color = "#475569" } },
						ui.text { "red", style = { color = "#dc2626" } },
					},
					ui.row {
						style = { gap = 8 },
						ui.text { "scroll row 6", style = { color = "#475569" } },
						ui.text { "cyan", style = { color = "#0891b2" } },
					},
					ui.row {
						style = { gap = 8 },
						ui.text { "scroll row 7", style = { color = "#475569" } },
						ui.text { "pink", style = { color = "#db2777" } },
					},
					ui.row {
						style = { gap = 8 },
						ui.text { "scroll row 8", style = { color = "#475569" } },
						ui.text { "slate", style = { color = "#334155" } },
					},
				},
			},

			ui.text { "Horizontal scroll", style = { color = "#0f172a" } },
			ui.scroll_view {
				style = {
					height = 96,
					width = "full",
					bg_color = "#f8fafc",
					border_color = "#94a3b8",
					border_width = 1,
				},
				ui.row {
					style = { gap = 12, width = 780 },
					ui.column {
						style = {
							width = 180,
							padding = 10,
							bg_color = "#dbeafe",
							border_color = "#2563eb",
							border_width = 1,
						},
						ui.text { "card one", style = { color = "#1d4ed8" } },
						ui.text { "swipe horizontally", style = { color = "#2563eb" } },
					},
					ui.column {
						style = {
							width = 180,
							padding = 10,
							bg_color = "#dcfce7",
							border_color = "#16a34a",
							border_width = 1,
						},
						ui.text { "card two", style = { color = "#15803d" } },
						ui.text { "row wider than view", style = { color = "#16a34a" } },
					},
					ui.column {
						style = {
							width = 180,
							padding = 10,
							bg_color = "#f3e8ff",
							border_color = "#9333ea",
							border_width = 1,
						},
						ui.text { "card three", style = { color = "#7e22ce" } },
						ui.text { "native scroll", style = { color = "#9333ea" } },
					},
					ui.column {
						style = {
							width = 180,
							padding = 10,
							bg_color = "#ffedd5",
							border_color = "#ea580c",
							border_width = 1,
						},
						ui.text { "card four", style = { color = "#c2410c" } },
						ui.text { "end of row", style = { color = "#ea580c" } },
					},
				},
			},
		},
	}
end
