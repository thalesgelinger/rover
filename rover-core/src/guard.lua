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
		-- Schema/migration modifiers
		primary = false,
		auto = false,
		unique = false,
		nullable = true,
		references_table = nil,
		index_flag = false,

		required = function(self, msg)
			self.required = true
			self.nullable = false
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
		end,

		-- Schema modifiers
		primary = function(self)
			self.primary = true
			self.nullable = false
			return self
		end,

		auto = function(self)
			self.auto = true
			return self
		end,

		unique = function(self)
			self.unique = true
			return self
		end,

		nullable = function(self)
			self.nullable = true
			self.required = false
			return self
		end,

		references = function(self, table_col)
			self.references_table = table_col
			return self
		end,

		index = function(self)
			self.index_flag = true
			return self
		end,
	}
end

function Guard:string()
	return create_validator "string"
end

function Guard:number()
	return create_validator "number"
end

function Guard:integer()
	return create_validator "integer"
end

function Guard:boolean()
	return create_validator "boolean"
end

function Guard:array(element_validator)
	local v = create_validator "array"
	v.element = element_validator
	return v
end

function Guard:object(schema)
	local v = create_validator "object"
	v.schema = schema
	return v
end

-- Helper function to wrap validation in xpcall without stack traces
function Guard.validate(fn)
	local success, result = xpcall(fn, function(err)
		local err_str = tostring(err)
		-- Remove "runtime error: " prefix
		err_str = err_str:gsub("^runtime error: ", "")
		-- Remove stack traceback
		local stack_pos = err_str:find "\nstack traceback:"
		if stack_pos then
			err_str = err_str:sub(1, stack_pos - 1)
		end
		return err_str
	end)

	if not success then
		error(result, 0) -- Re-throw with level 0 (no additional stack info)
	end

	return result
end

return Guard
