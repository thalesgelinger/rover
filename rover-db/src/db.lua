-- Rover DB - Intent-Based Query Builder & ORM DSL
-- Code expresses WHAT you want, Rover decides HOW to execute

local DB = {}

-- ============================================================================
-- Internal: Deep copy utility
-- ============================================================================
local function deep_copy(obj, seen)
    if type(obj) ~= "table" then
        return obj
    end
    seen = seen or {}
    if seen[obj] then
        return seen[obj]
    end
    local copy = {}
    seen[obj] = copy
    for k, v in pairs(obj) do
        copy[deep_copy(k, seen)] = deep_copy(v, seen)
    end
    setmetatable(copy, getmetatable(obj))
    return copy
end

-- ============================================================================
-- ColumnRef: Reference to a column (db.users.id)
-- ============================================================================
local ColumnRef = {}
ColumnRef.__index = ColumnRef

function ColumnRef.new(table_name, column_name)
    local self = setmetatable({}, ColumnRef)
    self._type = "column_ref"
    self._table = table_name
    self._column = column_name
    return self
end

function ColumnRef:__tostring()
    return self._table .. "." .. self._column
end

-- ============================================================================
-- TableProxy: Proxy for dynamic table access (db.users)
-- ============================================================================
local TableProxy = {}

local function create_table_proxy(db_instance, table_name)
    local proxy = {
        _type = "table_proxy",
        _name = table_name,
        _db = db_instance,
    }

    local mt = {
        -- db.users.id -> ColumnRef
        __index = function(_, column_name)
            return ColumnRef.new(table_name, column_name)
        end,

        __tostring = function()
            return "TableProxy(" .. table_name .. ")"
        end,
    }

    setmetatable(proxy, mt)

    -- Add methods directly to proxy (not through metatable to avoid conflicts)
    proxy.find = function(_)
        return DB._create_query(db_instance, table_name)
    end

    proxy.insert = function(_, data)
        return DB._insert(db_instance, table_name, data)
    end

    proxy.update = function(_)
        return DB._create_update_query(db_instance, table_name)
    end

    proxy.delete = function(_)
        return DB._create_delete_query(db_instance, table_name)
    end

    proxy.sql = function(_)
        return DB._create_raw_query(db_instance, table_name)
    end

    return proxy
end

-- ============================================================================
-- Query: Immutable query object representing intent
-- ============================================================================
local Query = {}
Query.__index = Query

function Query.new(db_instance, table_name)
    local self = setmetatable({}, Query)
    self._type = "query"
    self._db = db_instance
    self._table = table_name
    self._filters = {}
    self._exists_clauses = {}
    self._merged_queries = {}
    self._group_by_cols = {}
    self._aggregates = {}
    self._having_filters = {}
    self._order_by = {}
    self._limit_val = nil
    self._offset_val = nil
    self._preloads = {}
    self._select_cols = nil -- nil means SELECT *
    return self
end

-- Clone to maintain immutability
function Query:_clone()
    return deep_copy(self)
end

-- ============================================================================
-- Query: Semantic Filters (by_<field>_<operator>)
-- ============================================================================

-- Operators mapping
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

-- Parse filter method name: by_<field>_<operator> or by_<field> (equals)
local function parse_filter_method(method_name)
    if not method_name:match("^by_") then
        return nil, nil
    end

    local rest = method_name:sub(4) -- Remove "by_"

    -- Try to match known operators from longest to shortest
    local ops_by_length = {}
    for op in pairs(OPERATORS) do
        table.insert(ops_by_length, op)
    end
    table.sort(ops_by_length, function(a, b)
        return #a > #b
    end)

    for _, op in ipairs(ops_by_length) do
        local pattern = "_" .. op .. "$"
        if rest:match(pattern) then
            local field = rest:sub(1, -(#op + 2)) -- Remove _<operator>
            return field, op
        end
    end

    -- No operator found, default to "equals"
    return rest, "equals"
end

-- Parse having filter: having_<agg>_<op> (e.g., having_count_bigger_than)
local function parse_having_method(method_name)
    if not method_name:match("^having_") then
        return nil, nil
    end

    local rest = method_name:sub(8) -- Remove "having_"

    -- Try to match known operators
    local ops_by_length = {}
    for op in pairs(OPERATORS) do
        table.insert(ops_by_length, op)
    end
    table.sort(ops_by_length, function(a, b)
        return #a > #b
    end)

    for _, op in ipairs(ops_by_length) do
        local pattern = "_" .. op .. "$"
        if rest:match(pattern) then
            local agg_name = rest:sub(1, -(#op + 2))
            return agg_name, op
        end
    end

    return nil, nil
end

-- Parse with_* for preloads: with_user -> "user"
local function parse_preload_method(method_name)
    if method_name:match("^with_") then
        return method_name:sub(6)
    end
    return nil
end

-- Dynamic method handler for Query
local query_mt = {
    __index = function(self, key)
        -- Check for existing methods first
        if Query[key] then
            return Query[key]
        end

        -- Check for by_<field>_<operator> pattern
        local field, operator = parse_filter_method(key)
        if field then
            return function(_, value)
                local new_query = self:_clone()
                table.insert(new_query._filters, {
                    field = field,
                    operator = operator,
                    value = value,
                })
                return new_query
            end
        end

        -- Check for having_<agg>_<op> pattern
        local agg_name, having_op = parse_having_method(key)
        if agg_name then
            return function(_, value)
                local new_query = self:_clone()
                table.insert(new_query._having_filters, {
                    aggregate = agg_name,
                    operator = having_op,
                    value = value,
                })
                return new_query
            end
        end

        -- Check for with_<relation> pattern (preloads)
        local relation = parse_preload_method(key)
        if relation then
            return function(_)
                local new_query = self:_clone()
                table.insert(new_query._preloads, relation)
                return new_query
            end
        end

        return nil
    end,

    __tostring = function(self)
        return "Query(" .. self._table .. ")"
    end,
}

function Query.new_with_mt(db_instance, table_name)
    local self = Query.new(db_instance, table_name)
    setmetatable(self, query_mt)
    return self
end

-- ============================================================================
-- Query: EXISTS (Correlated Subqueries)
-- ============================================================================

function Query:exists(subquery)
    local new_query = self:_clone()
    -- Store the subquery, correlation will be added via :on()
    new_query._pending_exists = subquery
    return new_query
end

function Query:on(col_a, col_b)
    local new_query = self:_clone()
    if new_query._pending_exists then
        table.insert(new_query._exists_clauses, {
            subquery = new_query._pending_exists,
            correlation = { col_a, col_b },
        })
        new_query._pending_exists = nil
    end
    return new_query
end

-- ============================================================================
-- Query: select() for subquery projections
-- ============================================================================

function Query:select(...)
    local new_query = self:_clone()
    local cols = { ... }
    new_query._select_cols = cols
    return new_query
end

-- ============================================================================
-- Query: OR via merge (Query Composition)
-- ============================================================================

function Query:merge(...)
    local new_query = self:_clone()
    local queries = { ... }
    for _, q in ipairs(queries) do
        table.insert(new_query._merged_queries, q)
    end
    return new_query
end

-- Alias for merge
Query.any_of = Query.merge

-- ============================================================================
-- Query: Grouping & Aggregates
-- ============================================================================

function Query:group_by(...)
    local new_query = self:_clone()
    local cols = { ... }
    for _, col in ipairs(cols) do
        table.insert(new_query._group_by_cols, col)
    end
    return new_query
end

function Query:agg(aggregates)
    local new_query = self:_clone()
    new_query._aggregates = aggregates
    return new_query
end

-- ============================================================================
-- Query: Ordering, Limit, Offset
-- ============================================================================

function Query:order_by(col, direction)
    local new_query = self:_clone()
    direction = direction or "ASC"
    table.insert(new_query._order_by, { col = col, direction = direction })
    return new_query
end

function Query:limit(n)
    local new_query = self:_clone()
    new_query._limit_val = n
    return new_query
end

function Query:offset(n)
    local new_query = self:_clone()
    new_query._offset_val = n
    return new_query
end

-- ============================================================================
-- Query: Execution Methods
-- ============================================================================

function Query:all()
    return DB._execute_query(self._db, self, "all")
end

function Query:first()
    local limited = self:limit(1)
    local results = DB._execute_query(self._db, limited, "first")
    if results and #results > 0 then
        return results[1]
    end
    return nil
end

function Query:count()
    return DB._execute_query(self._db, self, "count")
end

-- ============================================================================
-- Query: Inspection
-- ============================================================================

function Query:inspect()
    local info = {
        intent = "SELECT",
        table = self._table,
        filters = {},
        exists_clauses = {},
        merged_queries = {},
        group_by = {},
        aggregates = {},
        having = {},
        order_by = self._order_by,
        limit = self._limit_val,
        offset = self._offset_val,
        preloads = self._preloads,
    }

    -- Format filters
    for _, f in ipairs(self._filters) do
        table.insert(info.filters, {
            field = f.field,
            operator = f.operator,
            value = f.value,
        })
    end

    -- Format exists clauses
    for _, e in ipairs(self._exists_clauses) do
        local subquery_info = e.subquery:inspect()
        table.insert(info.exists_clauses, {
            subquery = subquery_info,
            correlation = {
                tostring(e.correlation[1]),
                tostring(e.correlation[2]),
            },
        })
    end

    -- Format merged queries
    for _, mq in ipairs(self._merged_queries) do
        table.insert(info.merged_queries, mq:inspect())
    end

    -- Format group by
    for _, col in ipairs(self._group_by_cols) do
        table.insert(info.group_by, tostring(col))
    end

    -- Format aggregates
    for name, agg in pairs(self._aggregates) do
        info.aggregates[name] = tostring(agg)
    end

    -- Format having
    for _, h in ipairs(self._having_filters) do
        table.insert(info.having, {
            aggregate = h.aggregate,
            operator = h.operator,
            value = h.value,
        })
    end

    -- Generate candidate SQL
    info.candidate_sql = DB._generate_sql(self)

    -- Lowering strategy hints
    info.lowering_strategy = DB._determine_strategy(self)

    return info
end

-- ============================================================================
-- UpdateQuery: For update operations
-- ============================================================================

local UpdateQuery = {}
UpdateQuery.__index = UpdateQuery

function UpdateQuery.new(db_instance, table_name)
    local self = setmetatable({}, UpdateQuery)
    self._type = "update_query"
    self._db = db_instance
    self._table = table_name
    self._filters = {}
    self._set_values = {}
    return self
end

function UpdateQuery:_clone()
    return deep_copy(self)
end

function UpdateQuery:set(values)
    local new_query = self:_clone()
    for k, v in pairs(values) do
        new_query._set_values[k] = v
    end
    return new_query
end

function UpdateQuery:exec()
    return DB._execute_update(self._db, self)
end

-- Dynamic filter methods for UpdateQuery
local update_query_mt = {
    __index = function(self, key)
        if UpdateQuery[key] then
            return UpdateQuery[key]
        end

        local field, operator = parse_filter_method(key)
        if field then
            return function(_, value)
                local new_query = self:_clone()
                table.insert(new_query._filters, {
                    field = field,
                    operator = operator,
                    value = value,
                })
                return new_query
            end
        end

        return nil
    end,
}

function UpdateQuery.new_with_mt(db_instance, table_name)
    local self = UpdateQuery.new(db_instance, table_name)
    setmetatable(self, update_query_mt)
    return self
end

-- ============================================================================
-- DeleteQuery: For delete operations
-- ============================================================================

local DeleteQuery = {}
DeleteQuery.__index = DeleteQuery

function DeleteQuery.new(db_instance, table_name)
    local self = setmetatable({}, DeleteQuery)
    self._type = "delete_query"
    self._db = db_instance
    self._table = table_name
    self._filters = {}
    return self
end

function DeleteQuery:_clone()
    return deep_copy(self)
end

function DeleteQuery:exec()
    return DB._execute_delete(self._db, self)
end

-- Dynamic filter methods for DeleteQuery
local delete_query_mt = {
    __index = function(self, key)
        if DeleteQuery[key] then
            return DeleteQuery[key]
        end

        local field, operator = parse_filter_method(key)
        if field then
            return function(_, value)
                local new_query = self:_clone()
                table.insert(new_query._filters, {
                    field = field,
                    operator = operator,
                    value = value,
                })
                return new_query
            end
        end

        return nil
    end,
}

function DeleteQuery.new_with_mt(db_instance, table_name)
    local self = DeleteQuery.new(db_instance, table_name)
    setmetatable(self, delete_query_mt)
    return self
end

-- ============================================================================
-- RawQuery: For SQL escape hatch
-- ============================================================================

local RawQuery = {}
RawQuery.__index = RawQuery

function RawQuery.new(db_instance, table_name)
    local self = setmetatable({}, RawQuery)
    self._type = "raw_query"
    self._db = db_instance
    self._table = table_name
    self._sql = nil
    self._params = {}
    return self
end

function RawQuery:raw(sql, params)
    local new_query = setmetatable({}, RawQuery)
    new_query._type = "raw_query"
    new_query._db = self._db
    new_query._table = self._table
    new_query._sql = sql
    new_query._params = params or {}
    return new_query
end

function RawQuery:exec()
    return DB._execute_raw(self._db, self)
end

function RawQuery:all()
    return DB._execute_raw(self._db, self)
end

function RawQuery:inspect()
    return {
        intent = "RAW SQL",
        sql = self._sql,
        params = self._params,
    }
end

-- ============================================================================
-- Aggregate Functions
-- ============================================================================

local AggregateExpr = {}
AggregateExpr.__index = AggregateExpr

function AggregateExpr.new(func, col)
    local self = setmetatable({}, AggregateExpr)
    self._type = "aggregate"
    self._func = func
    self._col = col
    return self
end

function AggregateExpr:__tostring()
    return self._func .. "(" .. tostring(self._col) .. ")"
end

-- Aggregate factory functions
function DB.sum(col)
    return AggregateExpr.new("SUM", col)
end

function DB.count(col)
    return AggregateExpr.new("COUNT", col)
end

function DB.avg(col)
    return AggregateExpr.new("AVG", col)
end

function DB.min(col)
    return AggregateExpr.new("MIN", col)
end

function DB.max(col)
    return AggregateExpr.new("MAX", col)
end

-- ============================================================================
-- SQL Generation (Lowering)
-- ============================================================================

local function escape_value(value)
    if value == nil then
        return "NULL"
    elseif type(value) == "string" then
        -- Simple escaping - in production, use prepared statements
        return "'" .. value:gsub("'", "''") .. "'"
    elseif type(value) == "number" then
        return tostring(value)
    elseif type(value) == "boolean" then
        return value and "1" or "0"
    elseif type(value) == "table" then
        if value._type == "query" then
            -- Subquery
            return "(" .. DB._generate_sql(value) .. ")"
        elseif value._type == "column_ref" then
            return value._table .. "." .. value._column
        else
            -- Array for IN clause
            local items = {}
            for _, v in ipairs(value) do
                table.insert(items, escape_value(v))
            end
            return "(" .. table.concat(items, ", ") .. ")"
        end
    end
    return "NULL"
end

local function generate_filter_sql(filter)
    local field = filter.field
    local op = filter.operator
    local value = filter.value
    local sql_op = OPERATORS[op]

    if op == "is_null" or op == "is_not_null" then
        return field .. " " .. sql_op
    elseif op == "contains" then
        return field .. " LIKE '%" .. value:gsub("'", "''") .. "%'"
    elseif op == "starts_with" then
        return field .. " LIKE '" .. value:gsub("'", "''") .. "%'"
    elseif op == "ends_with" then
        return field .. " LIKE '%" .. value:gsub("'", "''") .. "'"
    elseif op == "between" then
        return field .. " BETWEEN " .. escape_value(value[1]) .. " AND " .. escape_value(value[2])
    elseif op == "in_list" or op == "not_in_list" then
        return field .. " " .. sql_op .. " " .. escape_value(value)
    else
        return field .. " " .. sql_op .. " " .. escape_value(value)
    end
end

function DB._generate_sql(query)
    if query._type == "raw_query" then
        return query._sql
    end

    local parts = {}

    -- SELECT clause
    if query._aggregates and next(query._aggregates) then
        local select_parts = {}
        for name, agg in pairs(query._aggregates) do
            table.insert(select_parts, tostring(agg) .. " AS " .. name)
        end
        -- Add group by columns to select
        for _, col in ipairs(query._group_by_cols) do
            table.insert(select_parts, tostring(col))
        end
        table.insert(parts, "SELECT " .. table.concat(select_parts, ", "))
    elseif query._select_cols then
        local cols = {}
        for _, col in ipairs(query._select_cols) do
            table.insert(cols, tostring(col))
        end
        table.insert(parts, "SELECT " .. table.concat(cols, ", "))
    else
        table.insert(parts, "SELECT *")
    end

    -- FROM clause
    table.insert(parts, "FROM " .. query._table)

    -- WHERE clause
    local where_conditions = {}

    -- Regular filters
    for _, filter in ipairs(query._filters) do
        table.insert(where_conditions, generate_filter_sql(filter))
    end

    -- EXISTS clauses
    for _, exists_clause in ipairs(query._exists_clauses) do
        local subquery = exists_clause.subquery
        local corr = exists_clause.correlation
        -- Add correlation to subquery filters
        local corr_filter =
            tostring(corr[1]) .. " = " .. tostring(corr[2])
        local subquery_sql = DB._generate_sql(subquery)
        -- Insert correlation into WHERE of subquery
        if subquery_sql:find("WHERE") then
            subquery_sql = subquery_sql:gsub("WHERE", "WHERE " .. corr_filter .. " AND")
        else
            subquery_sql = subquery_sql .. " WHERE " .. corr_filter
        end
        table.insert(where_conditions, "EXISTS (" .. subquery_sql .. ")")
    end

    -- Merged queries (OR)
    if #query._merged_queries > 0 then
        local or_parts = {}
        for _, mq in ipairs(query._merged_queries) do
            -- Get the WHERE part of merged query
            local mq_conditions = {}
            for _, filter in ipairs(mq._filters) do
                table.insert(mq_conditions, generate_filter_sql(filter))
            end
            for _, exists_clause in ipairs(mq._exists_clauses) do
                local subquery = exists_clause.subquery
                local corr = exists_clause.correlation
                local corr_filter = tostring(corr[1]) .. " = " .. tostring(corr[2])
                local subquery_sql = DB._generate_sql(subquery)
                if subquery_sql:find("WHERE") then
                    subquery_sql = subquery_sql:gsub("WHERE", "WHERE " .. corr_filter .. " AND")
                else
                    subquery_sql = subquery_sql .. " WHERE " .. corr_filter
                end
                table.insert(mq_conditions, "EXISTS (" .. subquery_sql .. ")")
            end
            if #mq_conditions > 0 then
                table.insert(or_parts, "(" .. table.concat(mq_conditions, " AND ") .. ")")
            end
        end
        if #or_parts > 0 then
            table.insert(where_conditions, "(" .. table.concat(or_parts, " OR ") .. ")")
        end
    end

    if #where_conditions > 0 then
        table.insert(parts, "WHERE " .. table.concat(where_conditions, " AND "))
    end

    -- GROUP BY clause
    if #query._group_by_cols > 0 then
        local group_cols = {}
        for _, col in ipairs(query._group_by_cols) do
            table.insert(group_cols, tostring(col))
        end
        table.insert(parts, "GROUP BY " .. table.concat(group_cols, ", "))
    end

    -- HAVING clause
    if #query._having_filters > 0 then
        local having_conditions = {}
        for _, h in ipairs(query._having_filters) do
            local agg_expr = h.aggregate
            local sql_op = OPERATORS[h.operator]
            table.insert(having_conditions, agg_expr .. " " .. sql_op .. " " .. escape_value(h.value))
        end
        table.insert(parts, "HAVING " .. table.concat(having_conditions, " AND "))
    end

    -- ORDER BY clause
    if #query._order_by > 0 then
        local order_parts = {}
        for _, o in ipairs(query._order_by) do
            table.insert(order_parts, tostring(o.col) .. " " .. o.direction)
        end
        table.insert(parts, "ORDER BY " .. table.concat(order_parts, ", "))
    end

    -- LIMIT and OFFSET
    if query._limit_val then
        table.insert(parts, "LIMIT " .. query._limit_val)
    end

    if query._offset_val then
        table.insert(parts, "OFFSET " .. query._offset_val)
    end

    return table.concat(parts, " ")
end

function DB._determine_strategy(query)
    local strategies = {}

    if #query._exists_clauses > 0 then
        table.insert(strategies, "EXISTS (correlated subquery)")
    end

    for _, filter in ipairs(query._filters) do
        if filter.operator == "in_list" and type(filter.value) == "table" and filter.value._type == "query" then
            table.insert(strategies, "IN (subquery)")
        end
    end

    if #query._merged_queries > 0 then
        table.insert(strategies, "OR composition")
    end

    if #query._preloads > 0 then
        table.insert(strategies, "Preload via separate queries or JOIN (TBD at execution)")
    end

    if #strategies == 0 then
        table.insert(strategies, "Simple query")
    end

    return strategies
end

-- ============================================================================
-- Update SQL Generation
-- ============================================================================

function DB._generate_update_sql(query)
    local parts = { "UPDATE " .. query._table }

    -- SET clause
    local set_parts = {}
    for k, v in pairs(query._set_values) do
        table.insert(set_parts, k .. " = " .. escape_value(v))
    end
    table.insert(parts, "SET " .. table.concat(set_parts, ", "))

    -- WHERE clause
    if #query._filters > 0 then
        local where_conditions = {}
        for _, filter in ipairs(query._filters) do
            table.insert(where_conditions, generate_filter_sql(filter))
        end
        table.insert(parts, "WHERE " .. table.concat(where_conditions, " AND "))
    end

    return table.concat(parts, " ")
end

-- ============================================================================
-- Delete SQL Generation
-- ============================================================================

function DB._generate_delete_sql(query)
    local parts = { "DELETE FROM " .. query._table }

    -- WHERE clause
    if #query._filters > 0 then
        local where_conditions = {}
        for _, filter in ipairs(query._filters) do
            table.insert(where_conditions, generate_filter_sql(filter))
        end
        table.insert(parts, "WHERE " .. table.concat(where_conditions, " AND "))
    end

    return table.concat(parts, " ")
end

-- ============================================================================
-- Insert SQL Generation
-- ============================================================================

function DB._generate_insert_sql(table_name, data)
    local columns = {}
    local values = {}

    for k, v in pairs(data) do
        table.insert(columns, k)
        table.insert(values, escape_value(v))
    end

    return "INSERT INTO "
        .. table_name
        .. " ("
        .. table.concat(columns, ", ")
        .. ") VALUES ("
        .. table.concat(values, ", ")
        .. ")"
end

-- ============================================================================
-- DB Instance: Factory Methods
-- ============================================================================

function DB._create_query(db_instance, table_name)
    return Query.new_with_mt(db_instance, table_name)
end

function DB._create_update_query(db_instance, table_name)
    return UpdateQuery.new_with_mt(db_instance, table_name)
end

function DB._create_delete_query(db_instance, table_name)
    return DeleteQuery.new_with_mt(db_instance, table_name)
end

function DB._create_raw_query(db_instance, table_name)
    return RawQuery.new(db_instance, table_name)
end

-- ============================================================================
-- DB Instance: Execution (Placeholder - Rust will override these)
-- ============================================================================

-- These functions are placeholders that will be overridden by Rust
-- They contain the SQL generation logic for inspection

function DB._insert(db_instance, table_name, data)
    local sql = DB._generate_insert_sql(table_name, data)
    -- The actual execution is handled by Rust
    return db_instance._executor("insert", sql, table_name, data)
end

function DB._execute_query(db_instance, query, mode)
    local sql = DB._generate_sql(query)
    return db_instance._executor("query", sql, query, mode)
end

function DB._execute_update(db_instance, query)
    local sql = DB._generate_update_sql(query)
    return db_instance._executor("update", sql, query)
end

function DB._execute_delete(db_instance, query)
    local sql = DB._generate_delete_sql(query)
    return db_instance._executor("delete", sql, query)
end

function DB._execute_raw(db_instance, query)
    return db_instance._executor("raw", query._sql, query._params)
end

-- ============================================================================
-- DB Instance Creation
-- ============================================================================

function DB.connect(config)
    local instance = {
        _config = config or {},
        _executor = nil, -- Will be set by Rust
    }

    -- Metatable for dynamic table access: db.users -> TableProxy
    local mt = {
        __index = function(self, key)
            -- Check for built-in methods first
            if DB[key] then
                return DB[key]
            end
            -- Otherwise, return a table proxy
            return create_table_proxy(self, key)
        end,
    }

    setmetatable(instance, mt)
    return instance
end

-- ============================================================================
-- Module Export
-- ============================================================================

return DB
