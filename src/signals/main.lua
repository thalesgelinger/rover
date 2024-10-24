os.execute("clear")
local rover = require "signals"

local count = rover.signal(0)
local double = count * 2

local incremet = function()
    count.set(count.get() + 1)
end

print("Runs only once")

rover.effect(function()
    print("Double: " .. double.get())
end)


local text = rover.signal("A")

rover.component("Val: " .. double .. text)




-- USER INTERACTION

function UI(fn, n)
    for _ = 1, n, 1 do
        fn()
        os.execute("sleep 1")
    end
end

UI(function()
    incremet()
    text.set(text.get() .. "A")
end, 5)
