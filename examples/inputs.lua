local app = rover.app()

function app.init()
    return {
        count = 0,
        text_value = "",
        checked = false,
    }
end

function app.increment(state)
    return { count = state.count + 1, text_value = state.text_value, checked = state.checked }
end

function app.toggle(state)
    return { count = state.count, text_value = state.text_value, checked = not state.checked }
end

function app.update_text(state, value)
    return { count = state.count, text_value = value, checked = state.checked }
end

function app.render(state, actions)
    return rover.col {
        rover.text { "Input Components Demo" },
        rover.button { "Count: " .. state.count, on_click = actions.increment() },
        
        rover.input {
            value = state.text_value,
            placeholder = "Enter text...",
            on_change = actions.update_text,
        },
        
        rover.checkbox {
            checked = state.checked,
            on_click = actions.toggle(),
        },
        
        rover.badge { "New" },
        
        rover.separator {},
        
        rover.card {
            rover.card_header {
                rover.text { "Card Header" },
            },
            rover.text { "Card content goes here" },
            rover.card_footer {
                rover.button { "Action", on_click = actions.increment() },
            },
        },
    }
end

return app
