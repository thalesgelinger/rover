-- rover-db Unit Tests
-- Tests for the Intent-Based Query Builder & ORM DSL

-- Test framework
local tests_passed = 0
local tests_failed = 0

local function test(name, fn)
    local success, err = pcall(fn)
    if success then
        tests_passed = tests_passed + 1
        print("[PASS] " .. name)
    else
        tests_failed = tests_failed + 1
        print("[FAIL] " .. name .. ": " .. tostring(err))
    end
end

local function assert_equals(expected, actual, msg)
    if expected ~= actual then
        error((msg or "Assertion failed") .. ": expected " .. tostring(expected) .. ", got " .. tostring(actual))
    end
end

local function assert_true(value, msg)
    if not value then
        error((msg or "Assertion failed") .. ": expected true, got " .. tostring(value))
    end
end

local function assert_not_nil(value, msg)
    if value == nil then
        error((msg or "Assertion failed") .. ": expected non-nil value")
    end
end

local function assert_contains(str, pattern, msg)
    if not str:find(pattern, 1, true) then
        error((msg or "Assertion failed") .. ": '" .. str .. "' does not contain '" .. pattern .. "'")
    end
end

-- Load the db module directly for testing
local DB = dofile("src/db.lua")

print("========================================")
print("Running rover-db Unit Tests")
print("========================================\n")

-- ============================================================================
-- Phase 1: Core Tests
-- ============================================================================

print("--- Phase 1: Core Tests ---\n")

test("DB.connect creates a database instance", function()
    local db = DB.connect()
    assert_not_nil(db)
    assert_not_nil(db._config)
end)

test("DB instance allows dynamic table access", function()
    local db = DB.connect()
    local users_proxy = db.users
    assert_not_nil(users_proxy)
    assert_equals("table_proxy", users_proxy._type)
    assert_equals("users", users_proxy._name)
end)

test("TableProxy returns ColumnRef on column access", function()
    local db = DB.connect()
    local col_ref = db.users.id
    assert_equals("column_ref", col_ref._type)
    assert_equals("users", col_ref._table)
    assert_equals("id", col_ref._column)
end)

test("ColumnRef tostring works correctly", function()
    local db = DB.connect()
    local col_ref = db.users.id
    assert_equals("users.id", tostring(col_ref))
end)

-- ============================================================================
-- Phase 2: Query Object Tests
-- ============================================================================

print("\n--- Phase 2: Query Object Tests ---\n")

test("find() creates a Query object", function()
    local db = DB.connect()
    local query = db.users:find()
    assert_equals("query", query._type)
    assert_equals("users", query._table)
end)

test("Query is immutable (chaining creates new objects)", function()
    local db = DB.connect()
    local query1 = db.users:find()
    local query2 = query1:by_age(25)

    assert_equals(0, #query1._filters)
    assert_equals(1, #query2._filters)
end)

test("by_<field> filter (default equals)", function()
    local db = DB.connect()
    local query = db.users:find():by_status("active")

    assert_equals(1, #query._filters)
    assert_equals("status", query._filters[1].field)
    assert_equals("equals", query._filters[1].operator)
    assert_equals("active", query._filters[1].value)
end)

test("by_<field>_bigger_than filter", function()
    local db = DB.connect()
    local query = db.users:find():by_age_bigger_than(18)

    assert_equals(1, #query._filters)
    assert_equals("age", query._filters[1].field)
    assert_equals("bigger_than", query._filters[1].operator)
    assert_equals(18, query._filters[1].value)
end)

test("by_<field>_smaller_than filter", function()
    local db = DB.connect()
    local query = db.users:find():by_age_smaller_than(65)

    assert_equals(1, #query._filters)
    assert_equals("age", query._filters[1].field)
    assert_equals("smaller_than", query._filters[1].operator)
end)

test("by_<field>_contains filter", function()
    local db = DB.connect()
    local query = db.users:find():by_name_contains("ana")

    assert_equals("name", query._filters[1].field)
    assert_equals("contains", query._filters[1].operator)
    assert_equals("ana", query._filters[1].value)
end)

test("by_<field>_starts_with filter", function()
    local db = DB.connect()
    local query = db.users:find():by_email_starts_with("admin")

    assert_equals("email", query._filters[1].field)
    assert_equals("starts_with", query._filters[1].operator)
end)

test("by_<field>_ends_with filter", function()
    local db = DB.connect()
    local query = db.users:find():by_email_ends_with(".com")

    assert_equals("email", query._filters[1].field)
    assert_equals("ends_with", query._filters[1].operator)
end)

test("by_<field>_in_list filter", function()
    local db = DB.connect()
    local query = db.users:find():by_status_in_list({"active", "pending", "paid"})

    assert_equals("status", query._filters[1].field)
    assert_equals("in_list", query._filters[1].operator)
    assert_equals(3, #query._filters[1].value)
end)

test("by_<field>_not_in_list filter", function()
    local db = DB.connect()
    local query = db.users:find():by_status_not_in_list({"banned", "deleted"})

    assert_equals("status", query._filters[1].field)
    assert_equals("not_in_list", query._filters[1].operator)
end)

test("by_<field>_between filter", function()
    local db = DB.connect()
    local query = db.users:find():by_age_between({18, 65})

    assert_equals("age", query._filters[1].field)
    assert_equals("between", query._filters[1].operator)
    assert_equals(18, query._filters[1].value[1])
    assert_equals(65, query._filters[1].value[2])
end)

test("by_<field>_is_null filter", function()
    local db = DB.connect()
    local query = db.users:find():by_deleted_at_is_null()

    assert_equals("deleted_at", query._filters[1].field)
    assert_equals("is_null", query._filters[1].operator)
end)

test("by_<field>_is_not_null filter", function()
    local db = DB.connect()
    local query = db.users:find():by_email_is_not_null()

    assert_equals("email", query._filters[1].field)
    assert_equals("is_not_null", query._filters[1].operator)
end)

test("Multiple filters can be chained", function()
    local db = DB.connect()
    local query = db.users:find()
        :by_status("active")
        :by_age_bigger_than(18)
        :by_role("admin")

    assert_equals(3, #query._filters)
end)

-- ============================================================================
-- Phase 3: Subqueries and EXISTS Tests
-- ============================================================================

print("\n--- Phase 3: Subqueries and EXISTS Tests ---\n")

test("Subquery can be used with by_<field>_in", function()
    local db = DB.connect()

    local active_users = db.users:find():by_status("active")
    local query = db.posts:find():by_author_id_in_list(active_users)

    assert_equals("author_id", query._filters[1].field)
    assert_equals("in_list", query._filters[1].operator)
    assert_equals("query", query._filters[1].value._type)
end)

test("exists() with on() creates correlated subquery", function()
    local db = DB.connect()

    local query = db.users:find()
        :exists(db.orders:find():by_status("paid"))
        :on(db.orders.user_id, db.users.id)

    assert_equals(1, #query._exists_clauses)
    assert_equals("query", query._exists_clauses[1].subquery._type)
    assert_equals("orders", query._exists_clauses[1].subquery._table)
end)

test("select() for subquery projections", function()
    local db = DB.connect()

    local user_ids = db.orders:find()
        :by_status("paid")
        :select(db.orders.user_id)

    assert_not_nil(user_ids._select_cols)
    assert_equals(1, #user_ids._select_cols)
end)

-- ============================================================================
-- Phase 4: OR Composition Tests (merge)
-- ============================================================================

print("\n--- Phase 4: OR Composition Tests ---\n")

test("merge() combines queries with OR", function()
    local db = DB.connect()

    local admins = db.users:find():by_role("admin")
    local minors = db.users:find():by_age_smaller_than(18)

    local query = db.users:find():merge(admins, minors)

    assert_equals(2, #query._merged_queries)
end)

test("any_of() is alias for merge()", function()
    local db = DB.connect()

    local admins = db.users:find():by_role("admin")
    local minors = db.users:find():by_age_smaller_than(18)

    local query = db.users:find():any_of(admins, minors)

    assert_equals(2, #query._merged_queries)
end)

test("Complex OR with EXISTS", function()
    local db = DB.connect()

    local has_paid_orders = db.users:find()
        :exists(db.orders:find():by_status("paid"))
        :on(db.orders.user_id, db.users.id)

    local has_subscription = db.users:find()
        :exists(db.subscriptions:find():by_active(true))
        :on(db.subscriptions.user_id, db.users.id)

    local query = db.users:find():merge(has_paid_orders, has_subscription)

    assert_equals(2, #query._merged_queries)
    assert_equals(1, #query._merged_queries[1]._exists_clauses)
    assert_equals(1, #query._merged_queries[2]._exists_clauses)
end)

-- ============================================================================
-- Phase 5: Grouping & Aggregates Tests
-- ============================================================================

print("\n--- Phase 5: Grouping & Aggregates Tests ---\n")

test("group_by() adds grouping", function()
    local db = DB.connect()

    local query = db.orders:find()
        :group_by(db.orders.user_id)

    assert_equals(1, #query._group_by_cols)
end)

test("agg() adds aggregates", function()
    local db = DB.connect()

    local query = db.orders:find()
        :group_by(db.orders.user_id)
        :agg({
            total = DB.sum(db.orders.amount),
            order_count = DB.count(db.orders.id)
        })

    assert_not_nil(query._aggregates.total)
    assert_not_nil(query._aggregates.order_count)
end)

test("having_<agg>_<op> filters on aggregates", function()
    local db = DB.connect()

    local query = db.orders:find()
        :group_by(db.orders.user_id)
        :agg({
            total = DB.sum(db.orders.amount),
            order_count = DB.count(db.orders.id)
        })
        :having_order_count_bigger_than(5)

    assert_equals(1, #query._having_filters)
    assert_equals("order_count", query._having_filters[1].aggregate)
    assert_equals("bigger_than", query._having_filters[1].operator)
    assert_equals(5, query._having_filters[1].value)
end)

test("Aggregate functions create AggregateExpr", function()
    local db = DB.connect()

    local sum_expr = DB.sum(db.orders.amount)
    assert_equals("aggregate", sum_expr._type)
    assert_equals("SUM", sum_expr._func)

    local count_expr = DB.count(db.orders.id)
    assert_equals("COUNT", count_expr._func)

    local avg_expr = DB.avg(db.orders.amount)
    assert_equals("AVG", avg_expr._func)

    local min_expr = DB.min(db.orders.amount)
    assert_equals("MIN", min_expr._func)

    local max_expr = DB.max(db.orders.amount)
    assert_equals("MAX", max_expr._func)
end)

-- ============================================================================
-- Phase 6: Ordering, Limit, Offset Tests
-- ============================================================================

print("\n--- Phase 6: Ordering, Limit, Offset Tests ---\n")

test("order_by() adds ordering", function()
    local db = DB.connect()

    local query = db.users:find():order_by(db.users.created_at, "DESC")

    assert_equals(1, #query._order_by)
    assert_equals("DESC", query._order_by[1].direction)
end)

test("limit() adds limit", function()
    local db = DB.connect()

    local query = db.users:find():limit(10)

    assert_equals(10, query._limit_val)
end)

test("offset() adds offset", function()
    local db = DB.connect()

    local query = db.users:find():offset(20)

    assert_equals(20, query._offset_val)
end)

-- ============================================================================
-- Phase 7: Preloads (with_*) Tests
-- ============================================================================

print("\n--- Phase 7: Preloads Tests ---\n")

test("with_<relation>() adds preload", function()
    local db = DB.connect()

    local query = db.posts:find()
        :by_published(true)
        :with_user()
        :with_comments()

    assert_equals(2, #query._preloads)
    assert_equals("user", query._preloads[1])
    assert_equals("comments", query._preloads[2])
end)

-- ============================================================================
-- Phase 8: SQL Generation Tests
-- ============================================================================

print("\n--- Phase 8: SQL Generation Tests ---\n")

test("Basic SELECT generation", function()
    local db = DB.connect()
    local query = db.users:find()
    local sql = DB._generate_sql(query)

    assert_contains(sql, "SELECT *")
    assert_contains(sql, "FROM users")
end)

test("SELECT with equals filter", function()
    local db = DB.connect()
    local query = db.users:find():by_status("active")
    local sql = DB._generate_sql(query)

    assert_contains(sql, "WHERE status = 'active'")
end)

test("SELECT with bigger_than filter", function()
    local db = DB.connect()
    local query = db.users:find():by_age_bigger_than(18)
    local sql = DB._generate_sql(query)

    assert_contains(sql, "WHERE age > 18")
end)

test("SELECT with contains filter (LIKE)", function()
    local db = DB.connect()
    local query = db.users:find():by_name_contains("ana")
    local sql = DB._generate_sql(query)

    assert_contains(sql, "LIKE '%ana%'")
end)

test("SELECT with starts_with filter", function()
    local db = DB.connect()
    local query = db.users:find():by_email_starts_with("admin")
    local sql = DB._generate_sql(query)

    assert_contains(sql, "LIKE 'admin%'")
end)

test("SELECT with in_list filter", function()
    local db = DB.connect()
    local query = db.users:find():by_status_in_list({"active", "pending"})
    local sql = DB._generate_sql(query)

    assert_contains(sql, "status IN")
    assert_contains(sql, "'active'")
    assert_contains(sql, "'pending'")
end)

test("SELECT with between filter", function()
    local db = DB.connect()
    local query = db.users:find():by_age_between({18, 65})
    local sql = DB._generate_sql(query)

    assert_contains(sql, "BETWEEN 18 AND 65")
end)

test("SELECT with is_null filter", function()
    local db = DB.connect()
    local query = db.users:find():by_deleted_at_is_null()
    local sql = DB._generate_sql(query)

    assert_contains(sql, "deleted_at IS NULL")
end)

test("SELECT with ORDER BY", function()
    local db = DB.connect()
    local query = db.users:find():order_by(db.users.created_at, "DESC")
    local sql = DB._generate_sql(query)

    assert_contains(sql, "ORDER BY users.created_at DESC")
end)

test("SELECT with LIMIT and OFFSET", function()
    local db = DB.connect()
    local query = db.users:find():limit(10):offset(20)
    local sql = DB._generate_sql(query)

    assert_contains(sql, "LIMIT 10")
    assert_contains(sql, "OFFSET 20")
end)

test("SELECT with GROUP BY and aggregates", function()
    local db = DB.connect()
    local query = db.orders:find()
        :group_by(db.orders.user_id)
        :agg({
            total = DB.sum(db.orders.amount),
            order_count = DB.count(db.orders.id)
        })
    local sql = DB._generate_sql(query)

    assert_contains(sql, "GROUP BY orders.user_id")
    assert_contains(sql, "SUM(orders.amount)")
    assert_contains(sql, "COUNT(orders.id)")
end)

test("SELECT with HAVING", function()
    local db = DB.connect()
    local query = db.orders:find()
        :group_by(db.orders.user_id)
        :agg({
            order_count = DB.count(db.orders.id)
        })
        :having_order_count_bigger_than(5)
    local sql = DB._generate_sql(query)

    assert_contains(sql, "HAVING order_count > 5")
end)

test("SELECT with EXISTS subquery", function()
    local db = DB.connect()
    local query = db.users:find()
        :exists(db.orders:find():by_status("paid"))
        :on(db.orders.user_id, db.users.id)
    local sql = DB._generate_sql(query)

    assert_contains(sql, "EXISTS")
    assert_contains(sql, "orders.user_id = users.id")
end)

test("SELECT with OR via merge", function()
    local db = DB.connect()
    local admins = db.users:find():by_role("admin")
    local mods = db.users:find():by_role("moderator")
    local query = db.users:find():merge(admins, mods)
    local sql = DB._generate_sql(query)

    assert_contains(sql, "OR")
    assert_contains(sql, "role = 'admin'")
    assert_contains(sql, "role = 'moderator'")
end)

-- ============================================================================
-- Phase 9: Update Query Tests
-- ============================================================================

print("\n--- Phase 9: Update Query Tests ---\n")

test("update() creates UpdateQuery", function()
    local db = DB.connect()
    local query = db.users:update()

    assert_equals("update_query", query._type)
end)

test("UpdateQuery with set() and filter", function()
    local db = DB.connect()
    local query = db.users:update()
        :by_id(1)
        :set({ status = "inactive" })

    assert_equals(1, #query._filters)
    assert_equals("inactive", query._set_values.status)
end)

test("UPDATE SQL generation", function()
    local db = DB.connect()
    local query = db.users:update()
        :by_id(1)
        :set({ status = "inactive", updated_at = "2024-01-15" })
    local sql = DB._generate_update_sql(query)

    assert_contains(sql, "UPDATE users")
    assert_contains(sql, "SET")
    assert_contains(sql, "status = 'inactive'")
    assert_contains(sql, "WHERE id = 1")
end)

-- ============================================================================
-- Phase 10: Delete Query Tests
-- ============================================================================

print("\n--- Phase 10: Delete Query Tests ---\n")

test("delete() creates DeleteQuery", function()
    local db = DB.connect()
    local query = db.users:delete()

    assert_equals("delete_query", query._type)
end)

test("DeleteQuery with filter", function()
    local db = DB.connect()
    local query = db.users:delete():by_status("banned")

    assert_equals(1, #query._filters)
    assert_equals("status", query._filters[1].field)
end)

test("DELETE SQL generation", function()
    local db = DB.connect()
    local query = db.users:delete():by_status("banned")
    local sql = DB._generate_delete_sql(query)

    assert_contains(sql, "DELETE FROM users")
    assert_contains(sql, "WHERE status = 'banned'")
end)

-- ============================================================================
-- Phase 11: Insert SQL Generation Tests
-- ============================================================================

print("\n--- Phase 11: Insert SQL Generation Tests ---\n")

test("INSERT SQL generation", function()
    local sql = DB._generate_insert_sql("users", {
        name = "Alice",
        age = 30,
        active = true
    })

    assert_contains(sql, "INSERT INTO users")
    assert_contains(sql, "VALUES")
    assert_contains(sql, "'Alice'")
end)

-- ============================================================================
-- Phase 12: Raw SQL (Escape Hatch) Tests
-- ============================================================================

print("\n--- Phase 12: Raw SQL Tests ---\n")

test("sql() creates RawQuery", function()
    local db = DB.connect()
    local query = db.users:sql()

    assert_equals("raw_query", query._type)
end)

test("RawQuery with raw()", function()
    local db = DB.connect()
    local query = db.users:sql():raw("SELECT * FROM users WHERE age > 18")

    assert_equals("SELECT * FROM users WHERE age > 18", query._sql)
end)

test("RawQuery inspect()", function()
    local db = DB.connect()
    local query = db.users:sql():raw("SELECT * FROM users WHERE age > ?", {18})
    local info = query:inspect()

    assert_equals("RAW SQL", info.intent)
    assert_contains(info.sql, "SELECT * FROM users")
end)

-- ============================================================================
-- Phase 13: inspect() Tests
-- ============================================================================

print("\n--- Phase 13: inspect() Tests ---\n")

test("inspect() returns query info", function()
    local db = DB.connect()
    local query = db.users:find()
        :by_status("active")
        :by_age_bigger_than(18)
        :limit(10)
    local info = query:inspect()

    assert_equals("SELECT", info.intent)
    assert_equals("users", info.table)
    assert_equals(2, #info.filters)
    assert_equals(10, info.limit)
    assert_not_nil(info.candidate_sql)
end)

test("inspect() shows EXISTS clauses", function()
    local db = DB.connect()
    local query = db.users:find()
        :exists(db.orders:find():by_status("paid"))
        :on(db.orders.user_id, db.users.id)
    local info = query:inspect()

    assert_equals(1, #info.exists_clauses)
end)

test("inspect() shows lowering strategy", function()
    local db = DB.connect()
    local query = db.users:find()
        :exists(db.orders:find():by_status("paid"))
        :on(db.orders.user_id, db.users.id)
    local info = query:inspect()

    assert_not_nil(info.lowering_strategy)
    assert_true(#info.lowering_strategy > 0)
end)

-- ============================================================================
-- Summary
-- ============================================================================

print("\n========================================")
print("Test Results: " .. tests_passed .. " passed, " .. tests_failed .. " failed")
print("========================================")

if tests_failed > 0 then
    os.exit(1)
end

return { passed = tests_passed, failed = tests_failed }
