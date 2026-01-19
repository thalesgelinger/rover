//! Integration tests for rover-db
//!
//! These tests verify the complete Lua DSL works end-to-end with actual database operations.

use mlua::prelude::*;
use rover_db::create_db_module;

fn setup_lua() -> Lua {
    let lua = Lua::new();

    // Create rover.db module
    let rover = lua.create_table().unwrap();
    let db_module = create_db_module(&lua).unwrap();
    rover.set("db", db_module).unwrap();
    lua.globals().set("rover", rover).unwrap();

    lua
}

#[test]
fn test_connect_and_insert() {
    let lua = setup_lua();

    let result: LuaResult<LuaTable> = lua
        .load(
            r#"
        local db = rover.db.connect()
        local result = db.users:insert({
            name = "Alice",
            age = 30
        })
        return result
    "#,
        )
        .eval();

    assert!(result.is_ok(), "Insert should succeed");
    let table = result.unwrap();
    assert!(table.get::<bool>("success").unwrap());
    assert!(table.get::<i64>("id").unwrap() >= 1);
}

#[test]
fn test_find_and_filters() {
    let lua = setup_lua();

    let sql: String = lua
        .load(
            r#"
        local db = rover.db.connect()

        -- Build a query
        local query = db.users:find()
            :by_status("active")
            :by_age_bigger_than(18)

        -- Get the generated SQL via inspect
        return query:inspect().candidate_sql
    "#,
        )
        .eval()
        .unwrap();

    assert!(sql.contains("SELECT *"));
    assert!(sql.contains("FROM users"));
    assert!(sql.contains("status = 'active'"));
    assert!(sql.contains("age > 18"));
}

#[test]
fn test_contains_filter() {
    let lua = setup_lua();

    let sql: String = lua
        .load(
            r#"
        local db = rover.db.connect()
        local query = db.users:find():by_name_contains("ana")
        return query:inspect().candidate_sql
    "#,
        )
        .eval()
        .unwrap();

    assert!(sql.contains("LIKE '%ana%'"));
}

#[test]
fn test_in_list_filter() {
    let lua = setup_lua();

    let sql: String = lua
        .load(
            r#"
        local db = rover.db.connect()
        local query = db.users:find():by_status_in_list({"active", "pending", "paid"})
        return query:inspect().candidate_sql
    "#,
        )
        .eval()
        .unwrap();

    assert!(sql.contains("status IN"));
    assert!(sql.contains("'active'"));
    assert!(sql.contains("'pending'"));
    assert!(sql.contains("'paid'"));
}

#[test]
fn test_between_filter() {
    let lua = setup_lua();

    let sql: String = lua
        .load(
            r#"
        local db = rover.db.connect()
        local query = db.users:find():by_age_between({18, 65})
        return query:inspect().candidate_sql
    "#,
        )
        .eval()
        .unwrap();

    assert!(sql.contains("BETWEEN 18 AND 65"));
}

#[test]
fn test_is_null_filter() {
    let lua = setup_lua();

    let sql: String = lua
        .load(
            r#"
        local db = rover.db.connect()
        local query = db.users:find():by_deleted_at_is_null()
        return query:inspect().candidate_sql
    "#,
        )
        .eval()
        .unwrap();

    assert!(sql.contains("deleted_at IS NULL"));
}

#[test]
fn test_exists_subquery() {
    let lua = setup_lua();

    let sql: String = lua
        .load(
            r#"
        local db = rover.db.connect()
        local query = db.users:find()
            :exists(db.orders:find():by_status("paid"))
            :on(db.orders.user_id, db.users.id)
        return query:inspect().candidate_sql
    "#,
        )
        .eval()
        .unwrap();

    assert!(sql.contains("EXISTS"));
    assert!(sql.contains("orders.user_id = users.id"));
    assert!(sql.contains("status = 'paid'"));
}

#[test]
fn test_or_composition_merge() {
    let lua = setup_lua();

    let sql: String = lua
        .load(
            r#"
        local db = rover.db.connect()
        local admins = db.users:find():by_role("admin")
        local mods = db.users:find():by_role("moderator")
        local query = db.users:find():merge(admins, mods)
        return query:inspect().candidate_sql
    "#,
        )
        .eval()
        .unwrap();

    assert!(sql.contains("OR"));
    assert!(sql.contains("role = 'admin'"));
    assert!(sql.contains("role = 'moderator'"));
}

#[test]
fn test_group_by_and_aggregates() {
    let lua = setup_lua();

    let sql: String = lua
        .load(
            r#"
        local db = rover.db.connect()
        local query = db.orders:find()
            :group_by(db.orders.user_id)
            :agg({
                total = rover.db.sum(db.orders.amount),
                order_count = rover.db.count(db.orders.id)
            })
        return query:inspect().candidate_sql
    "#,
        )
        .eval()
        .unwrap();

    assert!(sql.contains("GROUP BY orders.user_id"));
    assert!(sql.contains("SUM(orders.amount)"));
    assert!(sql.contains("COUNT(orders.id)"));
}

#[test]
fn test_having_clause() {
    let lua = setup_lua();

    let sql: String = lua
        .load(
            r#"
        local db = rover.db.connect()
        local query = db.orders:find()
            :group_by(db.orders.user_id)
            :agg({
                order_count = rover.db.count(db.orders.id)
            })
            :having_order_count_bigger_than(5)
        return query:inspect().candidate_sql
    "#,
        )
        .eval()
        .unwrap();

    assert!(sql.contains("HAVING order_count > 5"));
}

#[test]
fn test_order_by_limit_offset() {
    let lua = setup_lua();

    let sql: String = lua
        .load(
            r#"
        local db = rover.db.connect()
        local query = db.users:find()
            :order_by(db.users.created_at, "DESC")
            :limit(10)
            :offset(20)
        return query:inspect().candidate_sql
    "#,
        )
        .eval()
        .unwrap();

    assert!(sql.contains("ORDER BY users.created_at DESC"));
    assert!(sql.contains("LIMIT 10"));
    assert!(sql.contains("OFFSET 20"));
}

#[test]
fn test_update_query() {
    let lua = setup_lua();

    // Just verify the DSL works (can't easily test SQL generation for update without exposing it)
    let result: LuaResult<LuaTable> = lua
        .load(
            r#"
        local db = rover.db.connect()
        local query = db.users:update()
            :by_id(1)
            :set({ status = "inactive" })

        return {
            type = query._type,
            filter_count = #query._filters,
            has_set_values = query._set_values.status ~= nil
        }
    "#,
        )
        .eval();

    assert!(result.is_ok());
    let table = result.unwrap();
    assert_eq!(table.get::<String>("type").unwrap(), "update_query");
    assert_eq!(table.get::<i64>("filter_count").unwrap(), 1);
    assert!(table.get::<bool>("has_set_values").unwrap());
}

#[test]
fn test_delete_query() {
    let lua = setup_lua();

    let result: LuaResult<LuaTable> = lua
        .load(
            r#"
        local db = rover.db.connect()
        local query = db.users:delete():by_status("banned")

        return {
            type = query._type,
            filter_count = #query._filters
        }
    "#,
        )
        .eval();

    assert!(result.is_ok());
    let table = result.unwrap();
    assert_eq!(table.get::<String>("type").unwrap(), "delete_query");
    assert_eq!(table.get::<i64>("filter_count").unwrap(), 1);
}

#[test]
fn test_raw_sql_escape_hatch() {
    let lua = setup_lua();

    let info: LuaTable = lua
        .load(
            r#"
        local db = rover.db.connect()
        local query = db.users:sql():raw("SELECT * FROM users WHERE age > 18")
        return query:inspect()
    "#,
        )
        .eval()
        .unwrap();

    assert_eq!(info.get::<String>("intent").unwrap(), "RAW SQL");
    assert!(
        info.get::<String>("sql")
            .unwrap()
            .contains("SELECT * FROM users")
    );
}

#[test]
fn test_preloads() {
    let lua = setup_lua();

    let preloads: LuaTable = lua
        .load(
            r#"
        local db = rover.db.connect()
        local query = db.posts:find()
            :by_published(true)
            :with_author()
            :with_comments()
        return query:inspect().preloads
    "#,
        )
        .eval()
        .unwrap();

    assert_eq!(preloads.len().unwrap(), 2);
    assert_eq!(preloads.get::<String>(1).unwrap(), "author");
    assert_eq!(preloads.get::<String>(2).unwrap(), "comments");
}

#[test]
fn test_query_immutability() {
    let lua = setup_lua();

    let result: LuaTable = lua
        .load(
            r#"
        local db = rover.db.connect()
        local query1 = db.users:find()
        local query2 = query1:by_age(25)
        local query3 = query1:by_status("active")

        return {
            q1_filters = #query1._filters,
            q2_filters = #query2._filters,
            q3_filters = #query3._filters
        }
    "#,
        )
        .eval()
        .unwrap();

    // Query1 should remain unchanged
    assert_eq!(result.get::<i64>("q1_filters").unwrap(), 0);
    // Query2 and Query3 should each have 1 filter
    assert_eq!(result.get::<i64>("q2_filters").unwrap(), 1);
    assert_eq!(result.get::<i64>("q3_filters").unwrap(), 1);
}

#[test]
fn test_column_ref_tostring() {
    let lua = setup_lua();

    let col_str: String = lua
        .load(
            r#"
        local db = rover.db.connect()
        return tostring(db.users.id)
    "#,
        )
        .eval()
        .unwrap();

    assert_eq!(col_str, "users.id");
}

#[test]
fn test_aggregate_functions() {
    let lua = setup_lua();

    let result: LuaTable = lua
        .load(
            r#"
        local db = rover.db.connect()
        return {
            sum_type = rover.db.sum(db.orders.amount)._type,
            sum_func = rover.db.sum(db.orders.amount)._func,
            count_func = rover.db.count(db.orders.id)._func,
            avg_func = rover.db.avg(db.orders.amount)._func,
            min_func = rover.db.min(db.orders.amount)._func,
            max_func = rover.db.max(db.orders.amount)._func
        }
    "#,
        )
        .eval()
        .unwrap();

    assert_eq!(result.get::<String>("sum_type").unwrap(), "aggregate");
    assert_eq!(result.get::<String>("sum_func").unwrap(), "SUM");
    assert_eq!(result.get::<String>("count_func").unwrap(), "COUNT");
    assert_eq!(result.get::<String>("avg_func").unwrap(), "AVG");
    assert_eq!(result.get::<String>("min_func").unwrap(), "MIN");
    assert_eq!(result.get::<String>("max_func").unwrap(), "MAX");
}

#[test]
fn test_inspect_lowering_strategy() {
    let lua = setup_lua();

    let strategies: LuaTable = lua
        .load(
            r#"
        local db = rover.db.connect()

        -- Query with EXISTS
        local q1 = db.users:find()
            :exists(db.orders:find():by_status("paid"))
            :on(db.orders.user_id, db.users.id)

        return q1:inspect().lowering_strategy
    "#,
        )
        .eval()
        .unwrap();

    let first_strategy: String = strategies.get(1).unwrap();
    assert!(first_strategy.contains("EXISTS"));
}

#[test]
fn test_insert_and_query_roundtrip() {
    let lua = setup_lua();

    let count: i64 = lua
        .load(
            r#"
        local db = rover.db.connect()

        -- Insert test data
        db.test_users:insert({ name = "Test1", score = 100 })
        db.test_users:insert({ name = "Test2", score = 200 })
        db.test_users:insert({ name = "Test3", score = 150 })

        -- Query and count
        local results = db.test_users:find():by_score_bigger_than(120):all()
        return #results
    "#,
        )
        .eval()
        .unwrap();

    assert_eq!(count, 2); // Test2 (200) and Test3 (150)
}

#[test]
fn test_first_returns_single_record() {
    let lua = setup_lua();

    let result: LuaTable = lua
        .load(
            r#"
        local db = rover.db.connect()

        -- Insert test data
        db.first_test:insert({ name = "First", value = 1 })
        db.first_test:insert({ name = "Second", value = 2 })

        -- Get first record
        local first = db.first_test:find():order_by(db.first_test.value, "ASC"):first()
        return first
    "#,
        )
        .eval()
        .unwrap();

    assert_eq!(result.get::<String>("name").unwrap(), "First");
    assert_eq!(result.get::<i64>("value").unwrap(), 1);
}
