-- Comprehensive finite test for all UI components
local ru = rover.ui

function rover.render()
	-- Signals for testing
	local counter = rover.signal(0)
	local show_conditional = rover.signal(true)
	local items = rover.signal({ "first", "second" })

	-- Create a task that updates a few times then stops
	local tick = rover.task(function()
		rover.delay(50)
		counter.val = counter.val + 1

		rover.delay(50)
		show_conditional.val = false

		rover.delay(50)
		items.val = { "first", "second", "third" }

		rover.delay(50)
		counter.val = counter.val + 1
	end)

	tick()

	rover.on_destroy(function()
		rover.task.cancel(tick)
	end)

	-- Return comprehensive UI with all components
	return ru.column {
		-- Static text (should never update)
		ru.text { "Static header" },

		-- Dynamic text (should update twice)
		ru.text { "Count: " .. counter },

		-- Conditional rendering
		ru.when(show_conditional, function()
			return ru.text { "Conditional content: " .. counter }
		end),

		-- List rendering
		ru.text { "Items:" },
		ru.each(items, function(item, index)
			return ru.text { index .. ": " .. item }
		end, function(item, index)
			return item .. index
		end),

		-- Input component
		ru.input { value = "test input" },

		-- Checkbox
		ru.checkbox { checked = false },

		-- Image
		ru.image { src = "test.png" },

		-- Button
		ru.button {
			label = "Click me",
			on_click = function()
				counter.val = counter.val + 1
			end
		},

		-- Nested layout
		ru.row {
			ru.text { "Nested A" },
			ru.column {
				ru.text { "Nested B" },
				ru.view {
					ru.text { "Nested C" }
				}
			}
		},

		ru.text { "Static footer" }
	}
end
