local ui = rover.ui

function rover.render()
    local value = rover.signal(0)

    rover.interval(1000, function() 
        value.val = value.val + 1
    end)

    return ui.text {
        "Batata: "..value
    }
end
