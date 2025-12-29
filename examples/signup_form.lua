
local api = rover.server {}

local SignupForm = rover.component()

-- Initialize with empty state and no errors
function SignupForm.init()
    return {
        errors = {},
        submitted = false,
        success = false,
        data = {}
    }
end

-- Validation helpers
local function validateUsername(username)
    if username == "" or username == nil then
        return "Username is required"
    elseif #username < 3 then
        return "Username must be at least 3 characters"
    elseif #username > 20 then
        return "Username must be less than 20 characters"
    end
    return nil
end

local function validateEmail(email)
    if email == "" or email == nil then
        return "Email is required"
    elseif not string.match(email, "^[%w._%%-]+@[%w._%%-]+%.[%a]+$") then
        return "Please enter a valid email address"
    end
    return nil
end

local function validatePassword(password)
    if password == "" or password == nil then
        return "Password is required"
    elseif #password < 8 then
        return "Password must be at least 8 characters"
    elseif not string.match(password, "%d") then
        return "Password must contain at least one number"
    elseif not string.match(password, "%u") then
        return "Password must contain at least one uppercase letter"
    end
    return nil
end

local function validateConfirmPassword(password, confirmPassword)
    if confirmPassword == "" or confirmPassword == nil then
        return "Please confirm your password"
    elseif password ~= confirmPassword then
        return "Passwords do not match"
    end
    return nil
end

local function validateTerms(agreed)
    if not agreed then
        return "You must agree to the terms and conditions"
    end
    return nil
end

-- Handle form submission - receives form data as table
function SignupForm.submit(state, formData)
    local errors = {}

    -- Extract form values
    local username = formData.username or ""
    local email = formData.email or ""
    local password = formData.password or ""
    local confirmPassword = formData.confirmPassword or ""
    local agreeToTerms = formData.agreeToTerms or false

    -- Validate all fields
    local usernameError = validateUsername(username)
    if usernameError then errors.username = usernameError end

    local emailError = validateEmail(email)
    if emailError then errors.email = emailError end

    local passwordError = validatePassword(password)
    if passwordError then errors.password = passwordError end

    local confirmPasswordError = validateConfirmPassword(password, confirmPassword)
    if confirmPasswordError then errors.confirmPassword = confirmPasswordError end

    local termsError = validateTerms(agreeToTerms)
    if termsError then errors.terms = termsError end

    -- Count errors
    local hasErrors = false
    for _ in pairs(errors) do
        hasErrors = true
        break
    end

    return {
        errors = errors,
        submitted = true,
        success = not hasErrors,
        data = formData
    }
end

function SignupForm.reset(state)
    return {
        errors = {},
        submitted = false,
        success = false,
        data = {}
    }
end

function SignupForm.render(state)
    local data = {
        errors = state.errors,
        submitted = state.submitted,
        success = state.success,
        formData = state.data,
        hasErrors = state.submitted and not state.success
    }

    return rover.html(data) [=[
        <div style="max-width: 500px; margin: 40px auto; padding: 30px; background: white; border-radius: 8px; box-shadow: 0 2px 10px rgba(0,0,0,0.1); font-family: Arial, sans-serif;">
            <h2 style="text-align: center; color: #333; margin-bottom: 30px;">Create Account</h2>

            {{ if success then }}
                <!-- Success message -->
                <div style="padding: 20px; background: #d4edda; border: 1px solid #c3e6cb; border-radius: 6px; margin-bottom: 20px;">
                    <h3 style="margin: 0 0 10px 0; color: #155724;">✓ Account Created Successfully!</h3>
                    <p style="margin: 5px 0; color: #155724;"><strong>Username:</strong> {{ formData.username }}</p>
                    <p style="margin: 5px 0; color: #155724;"><strong>Email:</strong> {{ formData.email }}</p>
                    <button onclick="reset" style="margin-top: 15px; padding: 10px 20px; background: #28a745; color: white; border: none; border-radius: 4px; cursor: pointer; font-size: 14px;">Create Another Account</button>
                </div>
            {{ else }}
                {{ if hasErrors then }}
                    <!-- Error summary -->
                    <div style="padding: 15px; background: #f8d7da; border: 1px solid #f5c6cb; border-radius: 6px; margin-bottom: 20px;">
                        <h4 style="margin: 0 0 10px 0; color: #721c24;">Please fix the following errors:</h4>
                        <ul style="margin: 0; padding-left: 20px; color: #721c24;">
                            {{ if errors.username then }}<li>{{ errors.username }}</li>{{ end }}
                            {{ if errors.email then }}<li>{{ errors.email }}</li>{{ end }}
                            {{ if errors.password then }}<li>{{ errors.password }}</li>{{ end }}
                            {{ if errors.confirmPassword then }}<li>{{ errors.confirmPassword }}</li>{{ end }}
                            {{ if errors.terms then }}<li>{{ errors.terms }}</li>{{ end }}
                        </ul>
                    </div>
                {{ end }}

                <form id="signupForm" style="display: flex; flex-direction: column; gap: 20px;">
                    <!-- Username field -->
                    <div>
                        <label style="display: block; margin-bottom: 6px; font-weight: bold; color: #555;">Username *</label>
                        <input
                            type="text"
                            name="username"
                            value="{{ formData.username or '' }}"
                            placeholder="Choose a username"
                            style="width: 100%; padding: 10px; box-sizing: border-box; border: 2px solid {{ if errors.username then }}#f44336{{ else }}#ddd{{ end }}; border-radius: 4px; font-size: 14px;"
                        />
                        {{ if errors.username then }}
                            <small style="color: #f44336; margin-top: 4px; display: block;">{{ errors.username }}</small>
                        {{ end }}
                    </div>

                    <!-- Email field -->
                    <div>
                        <label style="display: block; margin-bottom: 6px; font-weight: bold; color: #555;">Email *</label>
                        <input
                            type="email"
                            name="email"
                            value="{{ formData.email or '' }}"
                            placeholder="your.email@example.com"
                            style="width: 100%; padding: 10px; box-sizing: border-box; border: 2px solid {{ if errors.email then }}#f44336{{ else }}#ddd{{ end }}; border-radius: 4px; font-size: 14px;"
                        />
                        {{ if errors.email then }}
                            <small style="color: #f44336; margin-top: 4px; display: block;">{{ errors.email }}</small>
                        {{ end }}
                    </div>

                    <!-- Password field -->
                    <div>
                        <label style="display: block; margin-bottom: 6px; font-weight: bold; color: #555;">Password *</label>
                        <input
                            type="password"
                            name="password"
                            placeholder="At least 8 characters"
                            style="width: 100%; padding: 10px; box-sizing: border-box; border: 2px solid {{ if errors.password then }}#f44336{{ else }}#ddd{{ end }}; border-radius: 4px; font-size: 14px;"
                        />
                        {{ if errors.password then }}
                            <small style="color: #f44336; margin-top: 4px; display: block;">{{ errors.password }}</small>
                        {{ else }}
                            <small style="color: #666; margin-top: 4px; display: block;">Must be 8+ characters with a number and uppercase letter</small>
                        {{ end }}
                    </div>

                    <!-- Confirm Password field -->
                    <div>
                        <label style="display: block; margin-bottom: 6px; font-weight: bold; color: #555;">Confirm Password *</label>
                        <input
                            type="password"
                            name="confirmPassword"
                            placeholder="Re-enter your password"
                            style="width: 100%; padding: 10px; box-sizing: border-box; border: 2px solid {{ if errors.confirmPassword then }}#f44336{{ else }}#ddd{{ end }}; border-radius: 4px; font-size: 14px;"
                        />
                        {{ if errors.confirmPassword then }}
                            <small style="color: #f44336; margin-top: 4px; display: block;">{{ errors.confirmPassword }}</small>
                        {{ end }}
                    </div>

                    <!-- Terms checkbox -->
                    <div style="display: flex; align-items: flex-start; gap: 10px;">
                        <input
                            type="checkbox"
                            name="agreeToTerms"
                            {{ if formData.agreeToTerms then }}checked{{ end }}
                            style="margin-top: 4px; cursor: pointer; width: 18px; height: 18px;"
                        />
                        <label style="flex: 1; cursor: pointer; color: #555; font-size: 14px;">
                            I agree to the Terms and Conditions and Privacy Policy *
                        </label>
                    </div>
                    {{ if errors.terms then }}
                        <small style="color: #f44336; margin-top: -12px; display: block;">{{ errors.terms }}</small>
                    {{ end }}

                    <!-- Submit button -->
                    <button
                        type="button"
                        onclick="submitForm(event, this)"
                        style="padding: 14px 24px; background: #007bff; color: white; border: none; border-radius: 6px; cursor: pointer; font-size: 16px; font-weight: bold; margin-top: 10px;"
                    >
                        Create Account
                    </button>

                    <p style="text-align: center; color: #999; font-size: 13px; margin: 0;">
                        * Required fields
                    </p>
                </form>

                <script>
                function submitForm(event, button) {
                    const form = document.getElementById('signupForm');
                    const formData = new FormData(form);

                    // Convert FormData to object
                    const data = {};
                    for (const [key, value] of formData.entries()) {
                        if (key === 'agreeToTerms') {
                            data[key] = true;
                        } else {
                            data[key] = value;
                        }
                    }

                    // If checkbox is not in FormData, it means it's unchecked
                    if (!formData.has('agreeToTerms')) {
                        data.agreeToTerms = false;
                    }

                    // Extract component ID from the button's closest rover component
                    const container = button.closest('[data-rover-component]');
                    const componentId = container.getAttribute('data-rover-component');

                    // Call rover event with form data
                    roverEvent(event, componentId, 'submit', data);
                }
                </script>
            {{ end }}
        </div>
    ]=]
end

function api.get()
    local data = { SignupForm = SignupForm }
    return api.html(data) [=[
        <!DOCTYPE html>
        <html>
        <head>
            <title>Sign Up - Form Validation Example</title>
            <meta name="viewport" content="width=device-width, initial-scale=1">
        </head>
        <body style="background: #f5f5f5; margin: 0; padding: 0;">
            {{ SignupForm() }}
        </body>
        </html>
    ]=]
end

return api
