local app = rover.server {}

function app.get()
    return app.html [[
        <h1>Hello World </h1>
    ]]
end

return app
