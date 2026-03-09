local count = rover.signal(0)
local doubled = rover.derive(function()
  return count.val * 2
end)

function rover.render()
  return rover.ui.column({
    rover.ui.text("Rover Web Counter"),
    rover.ui.row({
      rover.ui.text("count: " .. tostring(count)),
      rover.ui.text("double: " .. tostring(doubled)),
    }),
    rover.ui.button({
      "+1",
      on_click = function()
        count.val = count.val + 1
      end,
    }),
  })
end
