
local api = rover.server {}

local Counter = rover.component()

-- Props support: init receives props table
function Counter.init(props)
    local initial = props.initial or 0
    local step = props.step or 1
    return { count = initial, step = step }
end

function Counter.increase(state)
    return { count = state.count + state.step, step = state.step }
end

function Counter.decrease(state)
    return { count = state.count - state.step, step = state.step }
end

function Counter.render(state)
    local data = { count = state.count, step = state.step }
    return rover.html(data) [=[
        <div style="padding: 20px; border: 1px solid #ccc; margin: 10px;">
            <h2>Counter (step: {{ step }})</h2>
            <p style="font-size: 2em;">{{ count }}</p>
            <button onclick="increase">+ {{ step }}</button>
            <button onclick="decrease">- {{ step }}</button>
        </div>
    ]=]
end

function api.get()
    local data = { Counter = Counter }
    return api.html(data) [=[
        <!DOCTYPE html>
        <html>
        <head>
            <title>Counter with Props</title>
        </head>
        <body>
            <h1>Rover Component Props Demo</h1>

            <!-- Counter with default props (step=1, initial=0) -->
            {{ Counter() }}

            <!-- Counter starting at 100 with step 10 -->
            {{ Counter({ initial = 100, step = 10 }) }}

            <!-- Counter starting at -50 with step 5 -->
            {{ Counter({ initial = -50, step = 5 }) }}
        </body>
        </html>
    ]=]
end

return api
