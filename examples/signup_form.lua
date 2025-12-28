
local api = rover.server {}

local SignupForm = rover.component()

-- Initialize form with empty fields and no errors
function SignupForm.init()
    return {
        username = "",
        email = "",
        password = "",
        confirmPassword = "",
        agreeToTerms = false,
        errors = {},
        submitted = false,
        success = false
    }
end

-- Validation helpers
local function validateUsername(username)
    if username == "" then
        return "Username is required"
    elseif #username < 3 then
        return "Username must be at least 3 characters"
    elseif #username > 20 then
        return "Username must be less than 20 characters"
    end
    return nil
end

local function validateEmail(email)
    if email == "" then
        return "Email is required"
    elseif not string.match(email, "^[%w._%%-]+@[%w._%%-]+%.[%a]+$") then
        return "Please enter a valid email address"
    end
    return nil
end

local function validatePassword(password)
    if password == "" then
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
    if confirmPassword == "" then
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

-- Update field values
function SignupForm.updateUsername(state, value)
    return {
        username = value,
        email = state.email,
        password = state.password,
        confirmPassword = state.confirmPassword,
        agreeToTerms = state.agreeToTerms,
        errors = state.errors,
        submitted = false,
        success = false
    }
end

function SignupForm.updateEmail(state, value)
    return {
        username = state.username,
        email = value,
        password = state.password,
        confirmPassword = state.confirmPassword,
        agreeToTerms = state.agreeToTerms,
        errors = state.errors,
        submitted = false,
        success = false
    }
end

function SignupForm.updatePassword(state, value)
    return {
        username = state.username,
        email = state.email,
        password = value,
        confirmPassword = state.confirmPassword,
        agreeToTerms = state.agreeToTerms,
        errors = state.errors,
        submitted = false,
        success = false
    }
end

function SignupForm.updateConfirmPassword(state, value)
    return {
        username = state.username,
        email = state.email,
        password = state.password,
        confirmPassword = value,
        agreeToTerms = state.agreeToTerms,
        errors = state.errors,
        submitted = false,
        success = false
    }
end

function SignupForm.toggleTerms(state, checked)
    return {
        username = state.username,
        email = state.email,
        password = state.password,
        confirmPassword = state.confirmPassword,
        agreeToTerms = checked,
        errors = state.errors,
        submitted = false,
        success = false
    }
end

-- Validate and submit form
function SignupForm.submit(state)
    local errors = {}

    -- Validate all fields
    local usernameError = validateUsername(state.username)
    if usernameError then errors.username = usernameError end

    local emailError = validateEmail(state.email)
    if emailError then errors.email = emailError end

    local passwordError = validatePassword(state.password)
    if passwordError then errors.password = passwordError end

    local confirmPasswordError = validateConfirmPassword(state.password, state.confirmPassword)
    if confirmPasswordError then errors.confirmPassword = confirmPasswordError end

    local termsError = validateTerms(state.agreeToTerms)
    if termsError then errors.terms = termsError end

    -- Count errors
    local hasErrors = false
    for _ in pairs(errors) do
        hasErrors = true
        break
    end

    return {
        username = state.username,
        email = state.email,
        password = state.password,
        confirmPassword = state.confirmPassword,
        agreeToTerms = state.agreeToTerms,
        errors = errors,
        submitted = true,
        success = not hasErrors
    }
end

function SignupForm.reset(state)
    return {
        username = "",
        email = "",
        password = "",
        confirmPassword = "",
        agreeToTerms = false,
        errors = {},
        submitted = false,
        success = false
    }
end

function SignupForm.render(state)
    local data = {
        username = state.username,
        email = state.email,
        password = state.password,
        confirmPassword = state.confirmPassword,
        agreeToTerms = state.agreeToTerms,
        errors = state.errors,
        submitted = state.submitted,
        success = state.success,
        hasErrors = state.submitted and not state.success
    }

    return rover.html(data) [=[
        <div style="max-width: 500px; margin: 40px auto; padding: 30px; background: white; border-radius: 8px; box-shadow: 0 2px 10px rgba(0,0,0,0.1); font-family: Arial, sans-serif;">
            <h2 style="text-align: center; color: #333; margin-bottom: 30px;">Create Account</h2>

            {{ if success then }}
                <!-- Success message -->
                <div style="padding: 20px; background: #d4edda; border: 1px solid #c3e6cb; border-radius: 6px; margin-bottom: 20px;">
                    <h3 style="margin: 0 0 10px 0; color: #155724;">✓ Account Created Successfully!</h3>
                    <p style="margin: 5px 0; color: #155724;"><strong>Username:</strong> {{ username }}</p>
                    <p style="margin: 5px 0; color: #155724;"><strong>Email:</strong> {{ email }}</p>
                    <button onclick="reset" style="margin-top: 15px; padding: 10px 20px; background: #28a745; color: white; border: none; border-radius: 4px; cursor: pointer; font-size: 14px;">Create Another Account</button>
                </div>
            {{ elseif hasErrors then }}
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

            <form style="display: flex; flex-direction: column; gap: 20px;">
                <!-- Username field -->
                <div>
                    <label style="display: block; margin-bottom: 6px; font-weight: bold; color: #555;">Username *</label>
                    <input
                        type="text"
                        oninput="updateUsername"
                        value="{{ username }}"
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
                        oninput="updateEmail"
                        value="{{ email }}"
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
                        oninput="updatePassword"
                        value="{{ password }}"
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
                        oninput="updateConfirmPassword"
                        value="{{ confirmPassword }}"
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
                        onchange="toggleTerms"
                        {{ if agreeToTerms then }}checked{{ end }}
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
                    onclick="submit"
                    style="padding: 14px 24px; background: #007bff; color: white; border: none; border-radius: 6px; cursor: pointer; font-size: 16px; font-weight: bold; margin-top: 10px;"
                >
                    Create Account
                </button>

                <p style="text-align: center; color: #999; font-size: 13px; margin: 0;">
                    * Required fields
                </p>
            </form>
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
