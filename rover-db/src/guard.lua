-- Rover Guard - Zod-inspired validator for Lua
local Guard = {}

-- Helper to create chainable validator
local function create_validator(validator_type)
	return {
		_type = validator_type,
		_required = false,
		_required_msg = nil,
		_default = nil,
		_enum = nil,
		_element = nil,
		_schema = nil,
		-- Schema/migration modifiers
		_primary = false,
		_auto = false,
		_unique = false,
		_nullable = true,
		_references = nil,
		_index = false,

		required = function(self, msg)
			self._required = true
			self._nullable = false
			self._required_msg = msg
			return self
		end,

		default = function(self, value)
			self._default = value
			return self
		end,

		enum = function(self, values)
			self._enum = values
			return self
		end,

		-- Schema modifiers
		primary = function(self)
			self._primary = true
			self._nullable = false
			return self
		end,

		auto = function(self)
			self._auto = true
			return self
		end,

		unique = function(self)
			self._unique = true
			return self
		end,

		nullable = function(self)
			self._nullable = true
			self._required = false
			return self
		end,

		references = function(self, table_col)
			self._references = table_col
			return self
		end,

		index = function(self)
			self._index = true
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
	v._element = element_validator
	return v
end

function Guard:object(schema)
	local v = create_validator "object"
	v._schema = schema
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
