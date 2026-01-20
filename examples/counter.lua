local ru = rover.ui

local count = rover.signal(0)
local show_details = rover.signal(true)

local items = rover.signal {
	{ id = "a", label = "Alpha" },
	{ id = "b", label = "Beta" },
}

local function add_item()
	local list = items.val
	local next_id = string.char(97 + #list)
	local new = { id = next_id, label = "Item " .. next_id }
	local updated = {}
	for i = 1, #list do
		updated[i] = list[i]
	end
	updated[#list + 1] = new
	items.val = updated
end

return ru.column {
	ru.text { "Counter Demo" },
	ru.text { "Count: " .. count },
	ru.when(show_details, ru.text { "Details visible" }),
	ru.column {
		ru.text { "List" },
		ru.each(items, function(item)
			return ru.text { key = item.id, item.label }
		end),
	},
}
