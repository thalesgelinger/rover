-- Rover Schema DSL
-- Provides rover.schema.<table> { field = type } syntax

local SchemaDSL = {}

-- Global schema definitions (stored during load)
local schemas = {}

function SchemaDSL.register(table_name, definition)
    schemas[table_name] = definition
    return definition
end

function SchemaDSL.get_all()
    return schemas
end

function SchemaDSL.get(table_name)
    return schemas[table_name]
end

function SchemaDSL.clear()
    schemas = {}
end

-- Metatable for dynamic table access: rover.schema.users {...}
local schema_mt = {
    __index = function(self, table_name)
        return function(definition)
            SchemaDSL.register(table_name, definition)
            return definition
        end
    end,
}

setmetatable(SchemaDSL, schema_mt)

return SchemaDSL
