-- Code Analysis Engine
-- Scans Lua code to find db.* operations and infer schema

local Analyzer = {}

-- Infer Lua type from value
local function infer_type(value)
    if type(value) == "string" then
        return "text"
    elseif type(value) == "number" then
        if math.floor(value) == value then
            return "integer"
        else
            return "real"
        end
    elseif type(value) == "boolean" then
        return "integer" -- SQLite stores booleans as 0/1
    elseif value == nil then
        return "text" -- Default fallback
    else
        return "text"
    end
end

-- Extract table name and fields from db.<table>:insert({...})
local function analyze_insert_call(line)
    -- Pattern: db.<table>:insert({ ... })
    local table_name = line:match("db%.(%w+)%s*:%s*insert%s*%(%s*{")
    if not table_name then
        return nil
    end

    -- Extract the table definition (simplified - doesn't handle nested tables)
    local start_pos = line:find("{", line:find(":insert"))
    if not start_pos then
        return nil
    end

    local fields = {}
    -- Match key = value patterns
    for key, value_str in line:gmatch("(%w+)%s*=%s*([^,}]+)") do
        -- Try to parse value
        if value_str:match("^['\"]") then
            fields[key] = "text"
        elseif value_str:match("^%d+%.%d+") then
            fields[key] = "real"
        elseif value_str:match("^%d+$") then
            fields[key] = "integer"
        elseif value_str:match("^true$") or value_str:match("^false$") then
            fields[key] = "integer"
        else
            -- Reference to variable, assume text unless we can infer better
            fields[key] = "text"
        end
    end

    return {
        table = table_name,
        fields = fields,
    }
end

-- Analyze all Lua code (simple line-by-line scan)
function Analyzer.analyze_code(code)
    local tables = {}

    for line in code:gmatch("[^\n]+") do
        -- Skip comments
        if not line:match("^%s*%-%-") then
            -- Check for db.* operations
            if line:find("db%.%w+%s*:%s*insert") then
                local result = analyze_insert_call(line)
                if result then
                    if not tables[result.table] then
                        tables[result.table] = {}
                    end
                    -- Merge fields
                    for field_name, field_type in pairs(result.fields) do
                        if tables[result.table][field_name] then
                            -- Type already exists, verify compatibility
                            if tables[result.table][field_name] ~= field_type then
                                -- Type mismatch - will be caught during validation
                            end
                        else
                            tables[result.table][field_name] = field_type
                        end
                    end
                end
            end

            -- Check for db.* find/update/delete to infer referenced tables
            if line:find("db%.%w+%s*:%s*find") or
               line:find("db%.%w+%s*:%s*update") or
               line:find("db%.%w+%s*:%s*delete") then
                local table_name = line:match("db%.(%w+)%s*:")
                if table_name and not tables[table_name] then
                    tables[table_name] = {}
                end
            end
        end
    end

    return tables
end

-- Analyze directory recursively for all .lua files
function Analyzer.analyze_directory(directory)
    local all_tables = {}

    -- Helper to read files (stub - actual implementation uses io module)
    local function scan_dir(dir)
        -- This would be implemented in Rust to handle dir traversal
        -- For now, return empty
        return {}
    end

    return all_tables
end

return Analyzer
