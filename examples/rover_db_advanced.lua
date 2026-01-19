local db = rover.db.connect()

print("=== Setup: Create Data ===\n")

local u1 = db.users:insert({ name = "Alice", email = "alice@example.com", status = "active" })
local u2 = db.users:insert({ name = "Bob", email = "bob@example.com", status = "active" })
local u3 = db.users:insert({ name = "Charlie", email = "charlie@example.com", status = "inactive" })

local o1 = db.orders:insert({ user_id = u1.id, amount = 100, status = "paid" })
local o2 = db.orders:insert({ user_id = u1.id, amount = 50, status = "pending" })
local o3 = db.orders:insert({ user_id = u2.id, amount = 200, status = "paid" })
local o4 = db.orders:insert({ user_id = u3.id, amount = 150, status = "paid" })

db.posts:insert({ user_id = u1.id, title = "First Post", published = true })
db.posts:insert({ user_id = u1.id, title = "Draft Post", published = false })
db.posts:insert({ user_id = u2.id, title = "Alice Response", published = true })
db.posts:insert({ user_id = u3.id, title = "Charlie Thoughts", published = true })

print("✓ Data created\n")

print("=== Subqueries with IN ===\n")

local paid_order_users = db.orders:find()
    :by_status("paid")
    :select(db.orders.user_id)

local users_with_paid_orders = db.users:find()
    :by_id_in_list(paid_order_users)
    :all()

debug.print(users_with_paid_orders, "Users who have paid orders")

print("\n=== EXISTS with Correlated Subquery ===\n")

local users_with_orders = db.users:find()
    :exists(db.orders:find():by_status("paid"))
    :on(db.orders.user_id, db.users.id)
    :all()

debug.print(users_with_orders, "Users with paid orders (EXISTS)")

print("\n=== Complex OR with EXISTS ===\n")

local has_paid = db.users:find()
    :exists(db.orders:find():by_status("paid"))
    :on(db.orders.user_id, db.users.id)

local has_posts = db.users:find()
    :exists(db.posts:find():by_published(true))
    :on(db.posts.user_id, db.users.id)

local active_or_engaged = db.users:find():merge(has_paid, has_posts):all()
debug.print(active_or_engaged, "Users with paid orders OR published posts")

print("\n=== Preloads (with_*) ===\n")

local posts_data = db.posts:find()
    :by_published(true)
    :with_author()
    :all()

debug.print(posts_data, "Published posts with preload strategy")

print("\n=== Query Inspection: Generated SQL ===\n")

local complex = db.users:find()
    :by_status("active")
    :exists(db.orders:find():by_amount_bigger_than(100))
    :on(db.orders.user_id, db.users.id)
    :order_by(db.users.name, "ASC")
    :limit(5)

local info = complex:inspect()

print("Intent:", info.intent)
print("Table:", info.table)
print("Filters:", #info.filters)
print("EXISTS clauses:", #info.exists_clauses)
print("Lowering strategies:", table.concat(info.lowering_strategy, ", "))
print("\nCandidate SQL:")
print(info.candidate_sql)

print("\n=== Advanced Aggregates ===\n")

local stats = db.orders:find()
    :group_by(db.orders.user_id)
    :agg({
        total = rover.db.sum(db.orders.amount),
        avg = rover.db.avg(db.orders.amount),
        min = rover.db.min(db.orders.amount),
        max = rover.db.max(db.orders.amount),
        count = rover.db.count(db.orders.id)
    })
    :all()

debug.print(stats, "Order statistics by user")

print("\n=== HAVING with Multiple Aggregates ===\n")

local high_volume = db.orders:find()
    :group_by(db.orders.user_id)
    :agg({
        total = rover.db.sum(db.orders.amount),
        count = rover.db.count(db.orders.id)
    })
    :having_total_bigger_than(150)
    :having_count_bigger_than(1)
    :all()

debug.print(high_volume, "Users with total > 150 AND count > 1")

print("\n=== Multi-step Composition ===\n")

local mid_tier_users = db.users:find()
    :by_status("active")
    :exists(db.orders:find():by_amount_between({ 50, 200 }))
    :on(db.orders.user_id, db.users.id)
    :order_by(db.users.name, "ASC")

local mid_tier_info = mid_tier_users:inspect()
debug.print(mid_tier_info.candidate_sql, "Mid-tier users query")

print("\n=== Order Count per User ===\n")

local user_order_counts = db.orders:find()
    :group_by(db.orders.user_id)
    :agg({
        order_count = rover.db.count(db.orders.id),
        total_spent = rover.db.sum(db.orders.amount)
    })
    :order_by(db.orders.user_id, "ASC")
    :all()

debug.print(user_order_counts, "Order metrics per user")

print("\n=== Complete ===\n")
