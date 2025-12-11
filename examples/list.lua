local app = rover.app()

function app.init()
    local items = {}
    for i = 1, 2000 do
        table.insert(items, "Item " .. i)
    end
    return {
        selected = nil,
        items = items
    }
end

function app.select_item(state, id)
    return { selected = id, items = state.items }
end

function app.render(state, actions)
    return rover.col {
        rover.text { "List Demo", height = 40 },
        rover.separator { height = 1 },
        rover.scroll_area {
            height = "full",
            rover.list {
                data = state.items,
                render_item = function(index, item)
                    return rover.list_item {
                        item,
                        icon = "user",
                        on_click = actions.select_item(index),
                    }
                end,
                key = function(item) return item end,
            },
        },
    }
end

return app
