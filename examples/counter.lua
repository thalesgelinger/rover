
local api = rover.server {}

local Counter = rover.component()

function Counter.init()
    return 0
end

function Counter.increase(state)
    return state + 1
end

function Counter.render(state)
    local data = { value = data }
    return rover.html(data) [=[
        <h1> Counter {{ value }} </h1>
        <button onclick="increase">
    ]=]
end


function api.get()
    return api.html({}) [=[
        {{ Counter() }}
    ]=]
end

return api
