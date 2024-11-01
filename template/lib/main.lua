function rover.run()
    print("Batata")
    -- local signal = rover.signal(0)
    return rover.view {
        height = "full",
        width = "full",
        color = "#ffffff",
        rover.view {
            height = "100",
            width = "200",
            color = "#ff00ff",
        },
        rover.text {
            "Hello world"
        },
        rover.button {
            label = "Click me",
            onPress = function()
                print("On press this will be called")
            end
        }
    }
end
