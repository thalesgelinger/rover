-- Rover Guard - Zod-inspired validator for Lua
local Guard = {}

-- Helper to create chainable validator
local function create_validator(validator_type)
    return {
        type = validator_type,
        required = false,
        required_msg = nil,
        default = nil,
        enum = nil,
        element = nil,
        schema = nil,
        
        required = function(self, msg)
            self.required = true
            self.required_msg = msg
            return self
        end,
        
        default = function(self, value)
            self.default = value
            return self
        end,
        
        enum = function(self, values)
            self.enum = values
            return self
        end
    }
end

function Guard:string()
    return create_validator("string")
end

function Guard:number()
    return create_validator("number")
end

function Guard:integer()
    return create_validator("integer")
end

function Guard:boolean()
    return create_validator("boolean")
end

function Guard:array(element_validator)
    local v = create_validator("array")
    v.element = element_validator
    return v
end

function Guard:object(schema)
    local v = create_validator("object")
    v.schema = schema
    return v
end

return Guard
