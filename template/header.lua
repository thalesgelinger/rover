local function header(props)
    local value = rover.signal(0)

    rover.view {
        rover.text {
            props.text
        },
        rover.children(props)
    }
end

return header
