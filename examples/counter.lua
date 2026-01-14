
require "rover.ui"

local api = rover.server {}

local Counter = rover.component()

function Counter.init(props)
    return props.value
end

function Counter.increase(state)
	return state + 1
end

function Counter.render(state)
    return div {
        class = "", 
        h1 { "Counter " .. state },
        button { "Increase", onclick = self:increase }
    }
end

function api.get()
    return api.html_render({
        Counter { value = 0 }
    })
end

return api
