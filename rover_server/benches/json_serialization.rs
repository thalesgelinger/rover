use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use mlua::Lua;
use rover_server::to_json::ToJson;

fn create_simple_object(lua: &Lua) -> mlua::Table {
    let table = lua.create_table().unwrap();
    table.set("id", 12345).unwrap();
    table.set("name", "John Doe").unwrap();
    table.set("email", "john@example.com").unwrap();
    table.set("age", 30).unwrap();
    table.set("active", true).unwrap();
    table
}

fn create_complex_object(lua: &Lua) -> mlua::Table {
    let table = lua.create_table().unwrap();

    // User fields
    table.set("id", 12345).unwrap();
    table.set("username", "johndoe").unwrap();
    table.set("email", "john@example.com").unwrap();
    table.set("first_name", "John").unwrap();
    table.set("last_name", "Doe").unwrap();
    table.set("age", 30).unwrap();
    table.set("active", true).unwrap();
    table.set("verified", true).unwrap();
    table.set("role", "admin").unwrap();
    table.set("department", "Engineering").unwrap();

    // Nested address
    let address = lua.create_table().unwrap();
    address.set("street", "123 Main St").unwrap();
    address.set("city", "San Francisco").unwrap();
    address.set("state", "CA").unwrap();
    address.set("zip", "94105").unwrap();
    address.set("country", "USA").unwrap();
    table.set("address", address).unwrap();

    // Nested preferences
    let prefs = lua.create_table().unwrap();
    prefs.set("theme", "dark").unwrap();
    prefs.set("language", "en").unwrap();
    prefs.set("notifications", true).unwrap();
    prefs.set("timezone", "America/Los_Angeles").unwrap();
    table.set("preferences", prefs).unwrap();

    // Stats
    table.set("login_count", 1523).unwrap();
    table.set("last_login_timestamp", 1704067200).unwrap();
    table.set("created_at", 1609459200).unwrap();
    table.set("updated_at", 1704067200).unwrap();

    table
}

fn create_large_object(lua: &Lua, field_count: usize) -> mlua::Table {
    let table = lua.create_table().unwrap();

    for i in 0..field_count {
        let key = format!("field_{}", i);
        let value = format!("value_for_field_{}", i);
        table.set(key.as_str(), value.as_str()).unwrap();
    }

    table
}

fn create_array(lua: &Lua, size: usize) -> mlua::Table {
    let table = lua.create_table().unwrap();

    for i in 1..=size {
        table.set(i, format!("item_{}", i).as_str()).unwrap();
    }

    table
}

fn create_array_of_objects(lua: &Lua, size: usize) -> mlua::Table {
    let table = lua.create_table().unwrap();

    for i in 1..=size {
        let obj = lua.create_table().unwrap();
        obj.set("id", i as i64).unwrap();
        obj.set("name", format!("Item {}", i).as_str()).unwrap();
        obj.set("value", i as f64 * 1.5).unwrap();
        obj.set("active", i % 2 == 0).unwrap();
        table.set(i, obj).unwrap();
    }

    table
}

fn create_deeply_nested(lua: &Lua, depth: usize) -> mlua::Table {
    let mut current = lua.create_table().unwrap();
    current.set("value", depth as i64).unwrap();

    for i in (0..depth).rev() {
        let parent = lua.create_table().unwrap();
        parent.set("level", i as i64).unwrap();
        parent.set("child", current).unwrap();
        current = parent;
    }

    current
}

fn create_string_with_escapes(lua: &Lua) -> mlua::Table {
    let table = lua.create_table().unwrap();
    table.set("simple", "Hello World").unwrap();
    table.set("with_quotes", r#"She said "Hello""#).unwrap();
    table.set("with_newlines", "Line1\nLine2\nLine3").unwrap();
    table.set("with_tabs", "Col1\tCol2\tCol3").unwrap();
    table.set("mixed", "Text with \"quotes\"\nand newlines\tand tabs").unwrap();
    table.set("control_chars", "Text\x08with\x0Ccontrol\rchars").unwrap();
    table
}

fn bench_simple_object(c: &mut Criterion) {
    let lua = Lua::new();
    let table = create_simple_object(&lua);

    c.bench_function("json_simple_object", |b| {
        b.iter(|| {
            black_box(table.to_json_string().unwrap());
        });
    });
}

fn bench_complex_object(c: &mut Criterion) {
    let lua = Lua::new();
    let table = create_complex_object(&lua);

    c.bench_function("json_complex_object", |b| {
        b.iter(|| {
            black_box(table.to_json_string().unwrap());
        });
    });
}

fn bench_large_objects(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_large_objects");

    for size in [10, 50, 100, 200, 500].iter() {
        let lua = Lua::new();
        let table = create_large_object(&lua, *size);

        group.throughput(Throughput::Elements(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                black_box(table.to_json_string().unwrap());
            });
        });
    }

    group.finish();
}

fn bench_arrays(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_arrays");

    for size in [10, 50, 100, 500, 1000].iter() {
        let lua = Lua::new();
        let table = create_array(&lua, *size);

        group.throughput(Throughput::Elements(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                black_box(table.to_json_string().unwrap());
            });
        });
    }

    group.finish();
}

fn bench_array_of_objects(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_array_of_objects");

    for size in [10, 50, 100, 200].iter() {
        let lua = Lua::new();
        let table = create_array_of_objects(&lua, *size);

        group.throughput(Throughput::Elements(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                black_box(table.to_json_string().unwrap());
            });
        });
    }

    group.finish();
}

fn bench_nested_depth(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_nested_depth");

    for depth in [5, 10, 20, 32, 50].iter() {
        let lua = Lua::new();
        let table = create_deeply_nested(&lua, *depth);

        group.throughput(Throughput::Elements(*depth as u64));
        group.bench_with_input(BenchmarkId::from_parameter(depth), depth, |b, _| {
            b.iter(|| {
                black_box(table.to_json_string().unwrap());
            });
        });
    }

    group.finish();
}

fn bench_string_escaping(c: &mut Criterion) {
    let lua = Lua::new();
    let table = create_string_with_escapes(&lua);

    c.bench_function("json_string_escaping", |b| {
        b.iter(|| {
            black_box(table.to_json_string().unwrap());
        });
    });
}

fn bench_comparison_serde_json(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_comparison");

    // Benchmark our custom serializer
    let lua = Lua::new();
    let table = create_complex_object(&lua);

    group.bench_function("custom_serializer", |b| {
        b.iter(|| {
            black_box(table.to_json_string().unwrap());
        });
    });

    // Benchmark serde_json for comparison
    use serde_json::json;
    let serde_value = json!({
        "id": 12345,
        "username": "johndoe",
        "email": "john@example.com",
        "first_name": "John",
        "last_name": "Doe",
        "age": 30,
        "active": true,
        "verified": true,
        "role": "admin",
        "department": "Engineering",
        "address": {
            "street": "123 Main St",
            "city": "San Francisco",
            "state": "CA",
            "zip": "94105",
            "country": "USA"
        },
        "preferences": {
            "theme": "dark",
            "language": "en",
            "notifications": true,
            "timezone": "America/Los_Angeles"
        },
        "login_count": 1523,
        "last_login_timestamp": 1704067200,
        "created_at": 1609459200,
        "updated_at": 1704067200
    });

    group.bench_function("serde_json", |b| {
        b.iter(|| {
            black_box(serde_json::to_string(&serde_value).unwrap());
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_simple_object,
    bench_complex_object,
    bench_large_objects,
    bench_arrays,
    bench_array_of_objects,
    bench_nested_depth,
    bench_string_escaping,
    bench_comparison_serde_json
);

criterion_main!(benches);
