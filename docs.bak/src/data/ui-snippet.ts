export default `local ru = rover.ui

function rover.render()
    local count = rover.signal(0)

    return ru.column {
        ru.text { "Count: " .. count },
        ru.button {
            label = "Increase",
            on_click = function()
                count.val = count.val + 1
            end,
        },
    }
end`;
