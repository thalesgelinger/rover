-- Comprehensive UI test - exercises all components and verifies granular updates
local ru = rover.ui

function rover.render()
	-- Signals for different parts of the UI
	local counter = rover.signal(0)
	local show_extra = rover.signal(false)
	local items = rover.signal({ "apple", "banana" })
	local checked = rover.signal(false)
	local text_input = rover.signal("hello")
	local image_src = rover.signal("test.png")

	-- Task that updates counter every 200ms
	local tick = rover.task(function()
		while true do
			rover.delay(200)
			counter.val = counter.val + 1

			-- Toggle show_extra every 5 ticks (every 1 second)
			if counter.val % 5 == 0 then
				show_extra.val = not show_extra.val
			end

			-- Add item every 3 ticks (every 0.6 seconds)
			if counter.val % 3 == 0 then
				local current = items.val
				table.insert(current, "item_" .. counter.val)
				items.val = current
			end
		end
	end)

	tick()

	rover.on_destroy(function()
		rover.task.cancel(tick)
	end)

	-- Return comprehensive UI layout
	return ru.column {
		-- Static text (never updates - tests that static nodes aren't re-rendered)
		ru.text { "Static header (never updates)" },

		-- Dynamic counter (should update every 200ms)
		ru.text { "Count: " .. counter },

		-- Conditional rendering with rover.ui.when
		ru.when(show_extra, function()
			return ru.text { "Extra content visible! Count: " .. counter }
		end),

		-- Another conditional (should toggle)
		ru.when(show_extra, function()
			return ru.text { "More extra content!" }
		end),

		-- List rendering with rover.ui.each
		ru.text { "Fruits list:" },
		ru.each(items, function(item, index)
			return ru.row {
				ru.text { index .. ". " },
				ru.text { item }
			}
		end, function(item, index)
			return item .. index
		end),

		-- Input component (static for now)
		ru.text { "Input field:" },
		ru.input { value = text_input },

		-- Checkbox component
		ru.row {
			ru.text { "Checkbox:" },
			ru.checkbox { checked = checked }
		},

		-- Image component
		ru.text { "Image:" },
		ru.image { src = image_src },

		-- Button component
		ru.button {
			label = "Click me (count: " .. counter .. ")",
			on_click = function()
				checked.val = not checked.val
			end
		},

		-- Nested layout (column inside row inside column)
		ru.view {
			ru.row {
				ru.text { "Nested 1" },
				ru.column {
					ru.text { "Nested 2" },
					ru.text { "Nested 3" }
				}
			}
		},

		-- Footer with static text
		ru.text { "Static footer (never updates)" }
	}
end
