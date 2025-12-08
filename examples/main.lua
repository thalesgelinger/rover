local app = rover.app()

function app.init()
    return 0
end

function app.increase(state)
    return state + 1
end

function app.decrease(state)
    return state - 1
end

function app.render(s, act)
    return rover.col {
        height = 300,
        width = 'full',
        rover.text { "Count: " .. s },
        rover.row {
            rover.button { "Increase", on_click = act.increase() },
            rover.button { "Decrease", on_click = act.decrease() }
        }
    }
end

return app
