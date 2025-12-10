local function app()
  return { selected = nil }
end

function app.select_item(state, id)
  return { selected = id }
end

function app.render(state, actions)
  local items = {}
  for i = 1, 20 do
    local item_text = "Item " .. i
    table.insert(items, rover.list_item {
      item_text,
      icon = "user",
      on_click = actions.select_item,
    })
  end

  return rover.col {
    rover.text { "List Demo" },
    rover.separator {},
    rover.list(items),
  }
end

return app()
