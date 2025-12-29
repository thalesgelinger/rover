
local api = rover.server {}

local ContactForm = rover.component()

-- Initialize with empty state
function ContactForm.init()
    return {
        submitted = false,
        data = {}
    }
end

-- Handle form submission - receives all form data at once
function ContactForm.submit(state, formData)
    return {
        submitted = true,
        data = formData
    }
end

function ContactForm.reset(state)
    return {
        submitted = false,
        data = {}
    }
end

function ContactForm.render(state)
    local data = {
        submitted = state.submitted,
        formData = state.data
    }

    return rover.html(data) [=[
        <div style="padding: 20px; max-width: 500px; margin: 0 auto; font-family: Arial, sans-serif;">
            <h2>Contact Form</h2>

            {{ if submitted then }}
                <div style="padding: 15px; background: #d4edda; border: 1px solid #c3e6cb; border-radius: 4px; margin-bottom: 15px;">
                    <h3 style="margin: 0 0 10px 0; color: #155724;">✓ Form Submitted!</h3>
                    <p style="margin: 5px 0;"><strong>Name:</strong> {{ formData.name }}</p>
                    <p style="margin: 5px 0;"><strong>Email:</strong> {{ formData.email }}</p>
                    <p style="margin: 5px 0;"><strong>Message:</strong> {{ formData.message }}</p>
                    <p style="margin: 5px 0;"><strong>Subscribe:</strong> {{ if formData.subscribe then }}Yes{{ else }}No{{ end }}</p>
                    <button onclick="reset" style="margin-top: 10px; padding: 8px 16px; cursor: pointer; background: #28a745; color: white; border: none; border-radius: 4px;">Reset Form</button>
                </div>
            {{ else }}
                <form id="contactForm" style="display: flex; flex-direction: column; gap: 15px;">
                    <div>
                        <label style="display: block; margin-bottom: 5px; font-weight: bold;">Name:</label>
                        <input
                            type="text"
                            name="name"
                            value="{{ formData.name or '' }}"
                            placeholder="Enter your name"
                            style="width: 100%; padding: 8px; box-sizing: border-box; border: 1px solid #ccc; border-radius: 4px;"
                        />
                    </div>

                    <div>
                        <label style="display: block; margin-bottom: 5px; font-weight: bold;">Email:</label>
                        <input
                            type="email"
                            name="email"
                            value="{{ formData.email or '' }}"
                            placeholder="Enter your email"
                            style="width: 100%; padding: 8px; box-sizing: border-box; border: 1px solid #ccc; border-radius: 4px;"
                        />
                    </div>

                    <div>
                        <label style="display: block; margin-bottom: 5px; font-weight: bold;">Message:</label>
                        <textarea
                            name="message"
                            placeholder="Enter your message"
                            rows="4"
                            style="width: 100%; padding: 8px; box-sizing: border-box; border: 1px solid #ccc; border-radius: 4px;"
                        >{{ formData.message or '' }}</textarea>
                    </div>

                    <div style="display: flex; align-items: center; gap: 8px;">
                        <input
                            type="checkbox"
                            name="subscribe"
                            {{ if formData.subscribe then }}checked{{ end }}
                            style="cursor: pointer;"
                        />
                        <label style="margin: 0; cursor: pointer;">Subscribe to newsletter</label>
                    </div>

                    <button
                        type="button"
                        onclick="submitContactForm(event, this)"
                        style="padding: 10px 20px; background: #007bff; color: white; border: none; border-radius: 4px; cursor: pointer; font-size: 16px;"
                    >
                        Submit
                    </button>
                </form>

                <script>
                function submitContactForm(event, button) {
                    const form = document.getElementById('contactForm');
                    const formData = new FormData(form);

                    // Convert FormData to object
                    const data = {};
                    for (const [key, value] of formData.entries()) {
                        if (key === 'subscribe') {
                            data[key] = true;
                        } else {
                            data[key] = value;
                        }
                    }

                    // If checkbox not in FormData, it's unchecked
                    if (!formData.has('subscribe')) {
                        data.subscribe = false;
                    }

                    // Get component ID
                    const container = button.closest('[data-rover-component]');
                    const componentId = container.getAttribute('data-rover-component');

                    // Submit to server
                    roverEvent(event, componentId, 'submit', data);
                }
                </script>
            {{ end }}
        </div>
    ]=]
end

function api.get()
    local data = { ContactForm = ContactForm }
    return api.html(data) [=[
        <!DOCTYPE html>
        <html>
        <head>
            <title>Contact Form Demo</title>
        </head>
        <body style="background: #f5f5f5;">
            <h1 style="text-align: center; font-family: Arial, sans-serif; padding: 20px;">Rover Contact Form</h1>
            {{ ContactForm() }}
        </body>
        </html>
    ]=]
end

return api
