
local api = rover.server {}

local Counter = rover.component()

function Counter.init()
    return 0
end

function Counter.increase(state)
    return state + 1
end

function Counter.render(state)
    local data = { value = state }
    return rover.html(data) [=[
        <h1>Counter {{ value }}</h1>
        <button onclick="increase">Increase</button>
    ]=]
end


function api.get()
    local data = { Counter = Counter }
    return api.html(data) [=[
        <!DOCTYPE html>
        <html>
        <head>
            <title>Counter Example</title>
        </head>
        <body>
            <h1>Rover Stateful Component Demo</h1>
            {{ Counter() }}
        </body>
        </html>
    ]=]
end

return api
