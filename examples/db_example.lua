-- rover-db Integration Example
-- Demonstrates the Intent-Based Query Builder & ORM DSL

local db = rover.db.connect()

-- ============================================================================
-- Phase 1: Basic Insert & Query
-- ============================================================================

print("--- Basic Insert & Query ---")

-- Insert creates table if it doesn't exist (schema inference)
local result = db.users:insert({
    name = "Alice",
    age = 30,
    email = "alice@example.com",
    status = "active"
})
print("Inserted user with id:", result.id)

db.users:insert({ name = "Bob", age = 17, email = "bob@example.com", status = "active" })
db.users:insert({ name = "Charlie", age = 45, email = "charlie@example.com", status = "inactive" })
db.users:insert({ name = "Diana", age = 25, email = "diana@example.com", status = "active" })

-- Simple query
local active_users = db.users:find():by_status("active"):all()
print("Active users count:", #active_users)

-- Query with comparison
local adults = db.users:find():by_age_bigger_than(18):all()
print("Adults count:", #adults)

-- ============================================================================
-- Phase 2: Complex Filters
-- ============================================================================

print("\n--- Complex Filters ---")

-- Contains filter
local users_with_a = db.users:find():by_name_contains("a"):all()
print("Users with 'a' in name:", #users_with_a)

-- Multiple filters (AND)
local active_adults = db.users:find()
    :by_status("active")
    :by_age_bigger_than(18)
    :all()
print("Active adults:", #active_adults)

-- In list filter
local specific_statuses = db.users:find()
    :by_status_in_list({"active", "pending"})
    :all()
print("Active or pending users:", #specific_statuses)

-- Between filter
local age_range = db.users:find()
    :by_age_between({20, 40})
    :all()
print("Users aged 20-40:", #age_range)

-- ============================================================================
-- Phase 3: Orders & Relations
-- ============================================================================

print("\n--- Orders & Relations ---")

-- Create orders table
db.orders:insert({ user_id = 1, amount = 100.00, status = "paid" })
db.orders:insert({ user_id = 1, amount = 50.00, status = "pending" })
db.orders:insert({ user_id = 2, amount = 75.00, status = "paid" })
db.orders:insert({ user_id = 4, amount = 200.00, status = "paid" })

-- Subquery: users with paid orders
local paid_order_user_ids = db.orders:find()
    :by_status("paid")
    :select(db.orders.user_id)

local users_with_paid = db.users:find()
    :by_id_in_list(paid_order_user_ids)
local sql = users_with_paid:inspect().candidate_sql
print("Users with paid orders SQL:", sql)

-- EXISTS: users who have paid orders
local users_with_paid_orders = db.users:find()
    :exists(db.orders:find():by_status("paid"))
    :on(db.orders.user_id, db.users.id)
print("EXISTS query SQL:", users_with_paid_orders:inspect().candidate_sql)

-- ============================================================================
-- Phase 4: OR Composition (merge)
-- ============================================================================

print("\n--- OR Composition ---")

local admins = db.users:find():by_status("active")
local minors = db.users:find():by_age_smaller_than(18)

local admins_or_minors = db.users:find():merge(admins, minors)
print("OR query SQL:", admins_or_minors:inspect().candidate_sql)

-- Complex OR with EXISTS
local has_high_orders = db.users:find()
    :exists(db.orders:find():by_amount_bigger_than(100))
    :on(db.orders.user_id, db.users.id)

local is_inactive = db.users:find():by_status("inactive")

local special_users = db.users:find():merge(has_high_orders, is_inactive)
print("Complex OR SQL:", special_users:inspect().candidate_sql)

-- ============================================================================
-- Phase 5: Grouping & Aggregates
-- ============================================================================

print("\n--- Grouping & Aggregates ---")

local order_totals = db.orders:find()
    :group_by(db.orders.user_id)
    :agg({
        total = rover.db.sum(db.orders.amount),
        order_count = rover.db.count(db.orders.id)
    })

print("Aggregate query SQL:", order_totals:inspect().candidate_sql)

-- With HAVING
local big_spenders = db.orders:find()
    :group_by(db.orders.user_id)
    :agg({
        total = rover.db.sum(db.orders.amount),
        order_count = rover.db.count(db.orders.id)
    })
    :having_total_bigger_than(100)

print("HAVING query SQL:", big_spenders:inspect().candidate_sql)

-- ============================================================================
-- Phase 6: Ordering, Limit, Offset
-- ============================================================================

print("\n--- Ordering, Limit, Offset ---")

local recent_users = db.users:find()
    :order_by(db.users.id, "DESC")
    :limit(2)

print("Paginated query SQL:", recent_users:inspect().candidate_sql)

-- Pagination
local page2 = db.users:find()
    :order_by(db.users.id, "ASC")
    :limit(10)
    :offset(10)

print("Page 2 SQL:", page2:inspect().candidate_sql)

-- ============================================================================
-- Phase 7: Preloads (with_*)
-- ============================================================================

print("\n--- Preloads ---")

-- Create posts table for demo
db.posts:insert({ author_id = 1, title = "Hello World", published = true })
db.posts:insert({ author_id = 2, title = "My First Post", published = false })

local posts_with_author = db.posts:find()
    :by_published(true)
    :with_author()

local info = posts_with_author:inspect()
print("Preload strategy:", table.concat(info.lowering_strategy, ", "))
print("Preloads:", table.concat(info.preloads, ", "))

-- ============================================================================
-- Phase 8: Update & Delete
-- ============================================================================

print("\n--- Update & Delete ---")

-- Update
local update_sql = db.users:update()
    :by_id(1)
    :set({ status = "premium" })
print("UPDATE SQL:", update_sql:inspect and "not available" or "N/A")

-- Delete
local delete_sql = db.users:delete():by_status("banned")
print("DELETE (banned users) would execute")

-- ============================================================================
-- Phase 9: Raw SQL Escape Hatch
-- ============================================================================

print("\n--- Raw SQL Escape Hatch ---")

local raw_query = db.users:sql():raw([[
    SELECT u.*, COUNT(o.id) as order_count
    FROM users u
    LEFT JOIN orders o ON o.user_id = u.id
    GROUP BY u.id
    HAVING order_count > 0
]])

local raw_info = raw_query:inspect()
print("Raw SQL intent:", raw_info.intent)
print("Raw SQL:", raw_info.sql:sub(1, 50) .. "...")

-- ============================================================================
-- Phase 10: inspect() Deep Dive
-- ============================================================================

print("\n--- Query Inspection ---")

local complex_query = db.users:find()
    :by_status("active")
    :by_age_bigger_than(18)
    :exists(db.orders:find():by_status("paid"))
    :on(db.orders.user_id, db.users.id)
    :order_by(db.users.name, "ASC")
    :limit(10)

local inspection = complex_query:inspect()

print("Intent:", inspection.intent)
print("Table:", inspection.table)
print("Filters:", #inspection.filters)
print("EXISTS clauses:", #inspection.exists_clauses)
print("Limit:", inspection.limit)
print("Candidate SQL:", inspection.candidate_sql:sub(1, 80) .. "...")
print("Lowering strategies:", table.concat(inspection.lowering_strategy, ", "))

print("\n--- All Examples Complete ---")
