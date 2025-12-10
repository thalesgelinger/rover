local app = rover.app()

function app.init()
    return { progress = 0.5 }
end

function app.render(state, actions)
    return rover.col {
        rover.text { "Feedback Components" },
        
        rover.spinner {},
        
        rover.progress { value = tostring(state.progress) },
        
        rover.badge { "Beta" },
        rover.badge { "v1.0" },
        
        rover.avatar { "AB" },
        
        rover.separator {},
    }
end

return app
