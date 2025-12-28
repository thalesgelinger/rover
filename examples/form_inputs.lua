
local api = rover.server {}

local FormDemo = rover.component()

-- Initialize with empty form state
function FormDemo.init()
    return {
        name = "",
        email = "",
        message = "",
        subscribe = false,
        submitted = false
    }
end

-- Handle text input changes
function FormDemo.updateName(state, value)
    return {
        name = value,
        email = state.email,
        message = state.message,
        subscribe = state.subscribe,
        submitted = false
    }
end

function FormDemo.updateEmail(state, value)
    return {
        name = state.name,
        email = value,
        message = state.message,
        subscribe = state.subscribe,
        submitted = false
    }
end

function FormDemo.updateMessage(state, value)
    return {
        name = state.name,
        email = state.email,
        message = value,
        subscribe = state.subscribe,
        submitted = false
    }
end

-- Handle checkbox toggle
function FormDemo.toggleSubscribe(state, checked)
    return {
        name = state.name,
        email = state.email,
        message = state.message,
        subscribe = checked,
        submitted = false
    }
end

-- Handle form submission
function FormDemo.submit(state)
    return {
        name = state.name,
        email = state.email,
        message = state.message,
        subscribe = state.subscribe,
        submitted = true
    }
end

function FormDemo.reset(state)
    return {
        name = "",
        email = "",
        message = "",
        subscribe = false,
        submitted = false
    }
end

function FormDemo.render(state)
    local data = {
        name = state.name,
        email = state.email,
        message = state.message,
        subscribe = state.subscribe,
        submitted = state.submitted
    }

    return rover.html(data) [=[
        <div style="padding: 20px; max-width: 500px; margin: 0 auto; font-family: Arial, sans-serif;">
            <h2>Contact Form Demo</h2>

            {{ if submitted then }}
                <div style="padding: 15px; background: #d4edda; border: 1px solid #c3e6cb; border-radius: 4px; margin-bottom: 15px;">
                    <h3 style="margin: 0 0 10px 0; color: #155724;">Form Submitted!</h3>
                    <p style="margin: 5px 0;"><strong>Name:</strong> {{ name }}</p>
                    <p style="margin: 5px 0;"><strong>Email:</strong> {{ email }}</p>
                    <p style="margin: 5px 0;"><strong>Message:</strong> {{ message }}</p>
                    <p style="margin: 5px 0;"><strong>Subscribe:</strong> {{ if subscribe then }}Yes{{ else }}No{{ end }}</p>
                    <button onclick="reset" style="margin-top: 10px; padding: 8px 16px; cursor: pointer;">Reset Form</button>
                </div>
            {{ else }}
                <form style="display: flex; flex-direction: column; gap: 15px;">
                    <div>
                        <label style="display: block; margin-bottom: 5px; font-weight: bold;">Name:</label>
                        <input
                            type="text"
                            oninput="updateName"
                            value="{{ name }}"
                            placeholder="Enter your name"
                            style="width: 100%; padding: 8px; box-sizing: border-box; border: 1px solid #ccc; border-radius: 4px;"
                        />
                        <small style="color: #666;">Current: {{ name }}</small>
                    </div>

                    <div>
                        <label style="display: block; margin-bottom: 5px; font-weight: bold;">Email:</label>
                        <input
                            type="email"
                            oninput="updateEmail"
                            value="{{ email }}"
                            placeholder="Enter your email"
                            style="width: 100%; padding: 8px; box-sizing: border-box; border: 1px solid #ccc; border-radius: 4px;"
                        />
                        <small style="color: #666;">Current: {{ email }}</small>
                    </div>

                    <div>
                        <label style="display: block; margin-bottom: 5px; font-weight: bold;">Message:</label>
                        <textarea
                            oninput="updateMessage"
                            placeholder="Enter your message"
                            rows="4"
                            style="width: 100%; padding: 8px; box-sizing: border-box; border: 1px solid #ccc; border-radius: 4px;"
                        >{{ message }}</textarea>
                        <small style="color: #666;">Current: {{ message }}</small>
                    </div>

                    <div style="display: flex; align-items: center; gap: 8px;">
                        <input
                            type="checkbox"
                            onchange="toggleSubscribe"
                            {{ if subscribe then }}checked{{ end }}
                            style="cursor: pointer;"
                        />
                        <label style="margin: 0; cursor: pointer;">Subscribe to newsletter</label>
                    </div>

                    <button
                        type="button"
                        onclick="submit"
                        style="padding: 10px 20px; background: #007bff; color: white; border: none; border-radius: 4px; cursor: pointer; font-size: 16px;"
                    >
                        Submit
                    </button>
                </form>
            {{ end }}
        </div>
    ]=]
end

function api.get()
    local data = { FormDemo = FormDemo }
    return api.html(data) [=[
        <!DOCTYPE html>
        <html>
        <head>
            <title>Form Input Demo</title>
        </head>
        <body>
            <h1 style="text-align: center; font-family: Arial, sans-serif;">Rover Form Input Demo</h1>
            {{ FormDemo() }}
        </body>
        </html>
    ]=]
end

return api
