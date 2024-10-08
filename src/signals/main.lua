os.execute("clear")
local rover = require "signals"

local count = rover.signal(0)
local double = count * 2

print("Aqui só roda uma vez")

rover.effect(function()
    print("Inside effect: " .. double.get())
end)

local another = rover.signal(" Que")

rover.component("value " .. double .. another)

count.set(count.get() + 1)
count.set(count.get() + 1)
count.set(count.get() + 1)
count.set(count.get() + 1)
another.set(" Que foi")
