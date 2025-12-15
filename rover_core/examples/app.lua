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

function app.render(state)
    return rover.col {
        width = "full",
        height = 100,
        rover.text { "Count: " .. state },
        rover.row {
            rover.button { "Increase", press = "increase" },
            rover.button { "Decrease", press = "decrease" }
        }
    }
end
