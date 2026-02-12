require "rover.tui"

local ui = rover.ui
local PAGE_SIZE = 5

local function separator()
	local width = rover.signal(0)

	rover.interval(100, function()
		width.val = width.val + 1
	end)

	return ui.column {
		ui.separator { width = width, char = "-" },
        ui.text { width }
	}
end

function rover.render()
	local items = rover.signal {
		"buy milk",
		"write docs",
		"review PR",
		"ship patch",
		"walk dog",
		"book flights",
		"clean inbox",
	}

	local selected = rover.signal(1)
	local scroll_offset = rover.signal(1)
	local mode = rover.signal "normal"
	local show_input = rover.signal(false)
	local draft = rover.signal ""
	local status = rover.signal "up/down navigate | a add | e edit"

	local function list_count()
		return #items.val
	end

	local function ensure_visible()
		local n = list_count()
		if n == 0 then
			selected.val = 1
			scroll_offset.val = 1
			return
		end

		if selected.val < 1 then
			selected.val = 1
		elseif selected.val > n then
			selected.val = n
		end

		local max_offset = math.max(1, n - PAGE_SIZE + 1)
		if scroll_offset.val < 1 then
			scroll_offset.val = 1
		elseif scroll_offset.val > max_offset then
			scroll_offset.val = max_offset
		end

		if selected.val < scroll_offset.val then
			scroll_offset.val = selected.val
		elseif selected.val > scroll_offset.val + PAGE_SIZE - 1 then
			scroll_offset.val = selected.val - PAGE_SIZE + 1
		end
	end

	local function begin_add()
		mode.val = "adding"
		show_input.val = true
		draft.val = ""
		status.val = "add mode: type then Enter"
	end

	local function begin_edit()
		if list_count() == 0 then
			status.val = "nothing to edit"
			return
		end

		mode.val = "editing"
		show_input.val = true
		draft.val = tostring(items.val[selected.val] or "")
		status.val = "edit mode: type then Enter"
	end

	local function finish_input(text)
		local value = tostring(text or "")
		if value == "" then
			show_input.val = false
			mode.val = "normal"
			draft.val = ""
			status.val = "empty value ignored"
			return
		end

		local next = items.val
		if mode.val == "adding" then
			next[#next + 1] = value
			selected.val = #next
			status.val = "added: " .. value
		else
			next[selected.val] = value
			status.val = "updated item " .. tostring(selected.val)
		end

		items.val = next
		show_input.val = false
		mode.val = "normal"
		draft.val = ""
		ensure_visible()
	end

	local visible_items = rover.derive(function()
		local out = {}
		local start_idx = scroll_offset.val
		local stop_idx = math.min(start_idx + PAGE_SIZE - 1, #items.val)
		for i = start_idx, stop_idx do
			out[#out + 1] = items.val[i]
		end
		return out
	end)

	local selected_visible = rover.derive(function()
		local idx = selected.val - scroll_offset.val + 1
		local n = #visible_items.val
		if n == 0 then
			return 1
		end
		if idx < 1 then
			return 1
		end
		if idx > n then
			return n
		end
		return idx
	end)

	local range_text = rover.derive(function()
		local total = #items.val
		if total == 0 then
			return "0/0"
		end

		local start_idx = scroll_offset.val
		local stop_idx = math.min(start_idx + PAGE_SIZE - 1, total)
		return tostring(start_idx) .. "-" .. tostring(stop_idx) .. " / " .. tostring(total)
	end)

	return ui.column {
		ui.text { "todo app (simple)" },
		ui.badge { label = "a add | e edit", tone = "info" },
		ui.text { "visible: " .. range_text.val },
		separator(),

		ui.nav_list {
			title = "items",
			items = visible_items,
			selected = selected_visible,
			on_key = function(key)
				if mode.val ~= "normal" then
					return
				end

				if key == "up" or key == "char:k" then
					selected.val = selected.val - 1
					ensure_visible()
					return
				end
				if key == "down" or key == "char:j" then
					selected.val = selected.val + 1
					ensure_visible()
					return
				end
				if key == "home" then
					selected.val = 1
					ensure_visible()
					return
				end
				if key == "end" then
					selected.val = list_count()
					ensure_visible()
					return
				end
				if key == "char:a" then
					begin_add()
					return
				end
				if key == "char:e" then
					begin_edit()
					return
				end
			end,
		},

		ui.when(show_input, function()
			return ui.column {
				ui.separator { width = 50, char = "-" },
				ui.text {
					rover.derive(function()
						if mode.val == "adding" then
							return "new item"
						end
						return "edit item " .. tostring(selected.val)
					end),
				},
				ui.row {
					ui.text { "> " },
					ui.textarea {
						value = draft,
						on_change = function(val)
							draft.val = val
						end,
						on_submit = function(val)
							finish_input(val)
						end,
					},
				},
				ui.text { "Enter save (auto exit)" },
			}
		end),

		ui.separator { width = 50, char = "-" },
		ui.text { status },
	}
end
