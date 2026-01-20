local db = rover.db.connect()

print("=== CRUD Operations ===\n")

local user1 = db.users:insert({ name = "Alice", age = 30, email = "alice@example.com", status = "active" })
debug.print(user1, "Alice inserted")

local user2 = db.users:insert({ name = "Bob", age = 25, email = "bob@example.com", status = "active" })
local user3 = db.users:insert({ name = "Charlie", age = 17, email = "charlie@example.com", status = "inactive" })
local user4 = db.users:insert({ name = "Diana", age = 35, email = "diana@example.com", status = "active" })

print("\n=== Basic Filters ===\n")

local active = db.users:find():by_status("active"):all()
debug.print(active, "Active users")

local adults = db.users:find():by_age_bigger_than(18):all()
debug.print(adults, "Adults (age > 18)")

print("\n=== String Filters ===\n")

local contains_a = db.users:find():by_name_contains("a"):all()
debug.print(contains_a, "Names containing 'a'")

local emails_com = db.users:find():by_email_ends_with(".com"):all()
debug.print(emails_com, "Email ending with .com")

print("\n=== Range & List Filters ===\n")

local age_range = db.users:find():by_age_between({ 20, 35 }):all()
debug.print(age_range, "Users aged 20-35")

local statuses = db.users:find():by_status_in_list({ "active", "pending" }):all()
debug.print(statuses, "Active or pending users")

print("\n=== Insert Orders (using user IDs) ===\n")

local order1 = db.orders:insert({ user_id = user1.id, amount = 100.50, status = "paid" })
local order2 = db.orders:insert({ user_id = user1.id, amount = 50.25, status = "pending" })
local order3 = db.orders:insert({ user_id = user2.id, amount = 200.00, status = "paid" })
local order4 = db.orders:insert({ user_id = user4.id, amount = 75.99, status = "paid" })

debug.print(order1, "Order 1 created")

print("\n=== OR Composition (merge) ===\n")

local active_users = db.users:find():by_status("active")
local minors = db.users:find():by_age_smaller_than(18)
local active_or_minors = db.users:find():merge(active_users, minors):all()
debug.print(active_or_minors, "Active users OR minors")

print("\n=== Aggregates & Grouping ===\n")

local order_summary = db.orders:find()
    :group_by(db.orders.user_id)
    :agg({
        total = rover.db.sum(db.orders.amount),
        count = rover.db.count(db.orders.id)
    })
    :all()

debug.print(order_summary, "Orders grouped by user")

print("\n=== HAVING (Aggregate Filters) ===\n")

local big_spenders = db.orders:find()
    :group_by(db.orders.user_id)
    :agg({
        total = rover.db.sum(db.orders.amount),
        count = rover.db.count(db.orders.id)
    })
    :having_total_bigger_than(100)
    :all()

debug.print(big_spenders, "Users with orders total > 100")

print("\n=== Pagination ===\n")

local first_two = db.users:find()
    :order_by(db.users.name, "ASC")
    :limit(2)
    :all()

debug.print(first_two, "First 2 users (alphabetical)")

local page_two = db.users:find()
    :order_by(db.users.id, "DESC")
    :limit(2)
    :offset(2)
    :all()

debug.print(page_two, "Page 2 (2 users per page, DESC)")

print("\n=== Update ===\n")

db.users:update()
    :by_id(user3.id)
    :set({ status = "active", age = 18 })
    :exec()

local updated_user = db.users:find():by_id(user3.id):first()
debug.print(updated_user, "Charlie after update")

print("\n=== Delete ===\n")

db.orders:delete():by_status("pending"):exec()
local remaining_orders = db.orders:find():all()
debug.print(remaining_orders, "Orders after deleting pending")

print("\n=== Raw SQL Escape Hatch ===\n")

local raw_result = db.users:sql():raw([[
    SELECT name, COUNT(o.id) as order_count
    FROM users u
    LEFT JOIN orders o ON o.user_id = u.id
    GROUP BY u.id
    ORDER BY order_count DESC
]]):all()

debug.print(raw_result, "Users with order counts (raw SQL)")

print("\n=== Query Inspection ===\n")

local query = db.users:find()
    :by_status("active")
    :by_age_bigger_than(18)
    :order_by(db.users.name, "ASC")
    :limit(10)

local inspection = query:inspect()
debug.print(inspection.candidate_sql, "Generated SQL")

print("\n=== Complete ===\n")
