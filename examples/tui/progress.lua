local ui = rover.ui
local tui = rover.tui

function rover.render()
  local value = rover.signal(0)
  local max = 100
  local running = rover.signal(true)
  local status = rover.signal("running")

  local tick = rover.task(function()
    while true do
      rover.delay(120)
      if running.val then
        value.val = value.val + 1
        if value.val >= max then
          value.val = max
          running.val = false
          status.val = "done"
        end
      end
    end
  end)

  tick()

  rover.on_destroy(function()
    rover.task.cancel(tick)
  end)

  return ui.column {
    ui.text { "progress example" },
    tui.progress {
      label = "build",
      value = value,
      max = max,
      width = 36,
    },
    ui.text { status },
    ui.row {
      ui.button {
        label = "resume",
        on_click = function()
          running.val = true
          status.val = "running"
        end,
      },
      ui.button {
        label = "pause",
        on_click = function()
          running.val = false
          status.val = "paused"
        end,
      },
      ui.button {
        label = "reset",
        on_click = function()
          value.val = 0
          running.val = true
          status.val = "running"
        end,
      },
    },
  }
end
