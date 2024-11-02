function rover.run()
    print("Batata")
    return rover.view {
        height = "full",
        width = "full",
        color = "#ffffff",
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
