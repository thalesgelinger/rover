function rover.run()
    -- local signal = rover.signal(0)

    print("this is a log")

    return rover.view {
        height = "full",
        width = "100",
        color = "#0000ff",
        rover.view {
            height = "100",
            width = "full",
            color = "#00ff00"
        },
        rover.text {
            "Hello Rover",
        },
        rover.button {
            onPress = function()
                print("hello")
            end
        }
    }
end
