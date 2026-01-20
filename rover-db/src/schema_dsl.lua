-- Rover Schema DSL
-- Provides rover.schema.<table> { field = type } syntax
-- Generates schema-aware query methods when schemas are registered

local SchemaDSL = {}

-- Global schema definitions (stored during load)
local schemas = {}

-- Generated query methods per table (from schema fields)
local table_query_methods = {}

-- Operators mapping (mirrors db.lua OPERATORS)
local OPERATORS = {
    equals = "=",
    not_equals = "!=",
    bigger_than = ">",
    smaller_than = "<",
    bigger_than_or_equals = ">=",
    smaller_than_or_equals = "<=",
    contains = "LIKE",
    starts_with = "LIKE",
    ends_with = "LIKE",
    between = "BETWEEN",
    in_list = "IN",
    not_in_list = "NOT IN",
    is_null = "IS NULL",
    is_not_null = "IS NOT NULL",
}

-- Extract field names from a schema definition
-- Schema fields can be guard validators or simple values
local function extract_field_names(definition)
    local fields = {}
    for field_name, _ in pairs(definition) do
        -- Skip internal fields (starting with _)
        if type(field_name) == "string" and not field_name:match("^_") then
            table.insert(fields, field_name)
        end
    end
    return fields
end

-- Generate filter methods for a field
local function generate_field_methods(field_name)
    local methods = {}

    -- by_<field>(value) - equals
    methods["by_" .. field_name] = function(self, value)
        return self:_add_filter(field_name, "equals", value)
    end

    -- by_<field>_<operator>(value) - for each operator
    for op_name, _ in pairs(OPERATORS) do
        local method_name = "by_" .. field_name .. "_" .. op_name
        methods[method_name] = function(self, value)
            return self:_add_filter(field_name, op_name, value)
        end
    end

    return methods
end

-- Generate preload methods from relations
local function generate_preload_methods(definition)
    local methods = {}

    -- Check for _relations in definition
    if definition._relations then
        for relation_name, _ in pairs(definition._relations) do
            methods["with_" .. relation_name] = function(self)
                local new_query = self:_clone()
                table.insert(new_query._preloads, relation_name)
                return new_query
            end
        end
    end

    return methods
end

function SchemaDSL.register(table_name, definition)
    schemas[table_name] = definition

    -- Generate filter methods for each field
    local query_methods = {}
    local fields = extract_field_names(definition)

    for _, field_name in ipairs(fields) do
        local field_methods = generate_field_methods(field_name)
        for method_name, method_fn in pairs(field_methods) do
            query_methods[method_name] = method_fn
        end
    end

    -- Generate preload methods for relations
    local preload_methods = generate_preload_methods(definition)
    for method_name, method_fn in pairs(preload_methods) do
        query_methods[method_name] = method_fn
    end

    -- Store methods for this table
    table_query_methods[table_name] = query_methods

    return definition
end

function SchemaDSL.get_all()
    return schemas
end

function SchemaDSL.get(table_name)
    return schemas[table_name]
end

function SchemaDSL.get_query_methods(table_name)
    return table_query_methods[table_name]
end

function SchemaDSL.clear()
    schemas = {}
    table_query_methods = {}
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
