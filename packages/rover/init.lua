-- local ui = require "ui"

-- return ui.view {
--     backgroundColor = "#000fff",
--     alignItems = "center",
--     justifyContent = "center",
--
--     ui.text { "Hello World" },
--
--     ui.button {
--         "Say Hi",
--         onClick = function()
--             print "Hi"
--         end
--     }
-- }


function luaGreetings(name)
    return "Hello " .. name .. " your answer came from lua, BTW"
end
