use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use bytes::Bytes;
use smallvec::SmallVec;
use std::collections::HashMap;

// Simulate the request structure cloning that happens in event_loop.rs
type HeadersType = SmallVec<[(Bytes, Bytes); 8]>;
type QueryType = SmallVec<[(Bytes, Bytes); 8]>;
type ParamsType = HashMap<String, String>;

fn create_small_headers() -> HeadersType {
    let mut headers = SmallVec::new();
    headers.push((Bytes::from("content-type"), Bytes::from("application/json")));
    headers.push((Bytes::from("user-agent"), Bytes::from("Mozilla/5.0")));
    headers.push((Bytes::from("accept"), Bytes::from("*/*")));
    headers
}

fn create_large_headers() -> HeadersType {
    let mut headers = SmallVec::new();
    headers.push((Bytes::from("content-type"), Bytes::from("application/json")));
    headers.push((Bytes::from("user-agent"), Bytes::from("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36")));
    headers.push((Bytes::from("accept"), Bytes::from("application/json, text/plain, */*")));
    headers.push((Bytes::from("accept-encoding"), Bytes::from("gzip, deflate, br")));
    headers.push((Bytes::from("accept-language"), Bytes::from("en-US,en;q=0.9")));
    headers.push((Bytes::from("authorization"), Bytes::from("Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9")));
    headers.push((Bytes::from("cache-control"), Bytes::from("no-cache")));
    headers.push((Bytes::from("origin"), Bytes::from("https://example.com")));
    headers.push((Bytes::from("referer"), Bytes::from("https://example.com/dashboard")));
    headers.push((Bytes::from("x-requested-with"), Bytes::from("XMLHttpRequest")));
    headers.push((Bytes::from("x-api-key"), Bytes::from("sk_test_1234567890abcdef")));
    headers.push((Bytes::from("x-request-id"), Bytes::from("req_abc123def456")));
    headers
}

fn create_query_params(count: usize) -> QueryType {
    let mut query = SmallVec::new();
    for i in 0..count {
        let key = format!("param{}", i);
        let value = format!("value{}", i);
        query.push((Bytes::from(key), Bytes::from(value)));
    }
    query
}

fn create_route_params(count: usize) -> ParamsType {
    let mut params = HashMap::new();
    for i in 0..count {
        params.insert(format!("param{}", i), format!("value{}", i));
    }
    params
}

// Benchmark: Cloning headers (current approach in event_loop.rs:200)
fn bench_clone_headers(c: &mut Criterion) {
    let mut group = c.benchmark_group("clone_headers");

    let small = create_small_headers();
    group.bench_function("small_3_headers", |b| {
        b.iter(|| {
            let _clone = black_box(small.clone());
        });
    });

    let large = create_large_headers();
    group.bench_function("large_12_headers", |b| {
        b.iter(|| {
            let _clone = black_box(large.clone());
        });
    });

    group.finish();
}

// Benchmark: Cloning query parameters
fn bench_clone_query(c: &mut Criterion) {
    let mut group = c.benchmark_group("clone_query");

    for size in [0, 3, 8, 15].iter() {
        let query = create_query_params(*size);

        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                let _clone = black_box(query.clone());
            });
        });
    }

    group.finish();
}

// Benchmark: Cloning route params HashMap
fn bench_clone_params(c: &mut Criterion) {
    let mut group = c.benchmark_group("clone_params");

    for size in [0, 1, 3, 5].iter() {
        let params = create_route_params(*size);

        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                let _clone = black_box(params.clone());
            });
        });
    }

    group.finish();
}

// Benchmark: Full context cloning (4x clones as in event_loop.rs:200-203)
fn bench_full_context_clone(c: &mut Criterion) {
    let headers = create_large_headers();
    let query = create_query_params(5);
    let params = create_route_params(3);
    let body = Some(Bytes::from(r#"{"user_id": 123, "name": "Test User"}"#));

    c.bench_function("full_context_clone_4x", |b| {
        b.iter(|| {
            let _headers_clone = black_box(headers.clone());
            let _query_clone = black_box(query.clone());
            let _params_clone = black_box(params.clone());
            let _body_clone = black_box(body.clone());
        });
    });
}

// Benchmark: SmallVec inline vs heap allocation threshold
fn bench_smallvec_threshold(c: &mut Criterion) {
    let mut group = c.benchmark_group("smallvec_inline_threshold");

    // Test different sizes around the threshold (currently 8)
    for size in [4, 7, 8, 9, 12, 16].iter() {
        let query = create_query_params(*size);

        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                let _clone = black_box(query.clone());
            });
        });
    }

    group.finish();
}

// Benchmark: Bytes cloning cost
fn bench_bytes_clone(c: &mut Criterion) {
    let mut group = c.benchmark_group("bytes_clone");

    let small_bytes = Bytes::from("short");
    group.bench_function("small_5_bytes", |b| {
        b.iter(|| {
            let _clone = black_box(small_bytes.clone());
        });
    });

    let medium_bytes = Bytes::from("medium length string with some content");
    group.bench_function("medium_40_bytes", |b| {
        b.iter(|| {
            let _clone = black_box(medium_bytes.clone());
        });
    });

    let large_bytes = Bytes::from("a".repeat(1024));
    group.bench_function("large_1kb", |b| {
        b.iter(|| {
            let _clone = black_box(large_bytes.clone());
        });
    });

    let huge_bytes = Bytes::from("a".repeat(10 * 1024));
    group.bench_function("huge_10kb", |b| {
        b.iter(|| {
            let _clone = black_box(huge_bytes.clone());
        });
    });

    group.finish();
}

// Benchmark: String allocations (common in request parsing)
fn bench_string_allocations(c: &mut Criterion) {
    let mut group = c.benchmark_group("string_allocations");

    let source = "test_string_value";

    group.bench_function("to_string", |b| {
        b.iter(|| {
            let _s = black_box(source.to_string());
        });
    });

    group.bench_function("bytes_from_string", |b| {
        b.iter(|| {
            let _b = black_box(Bytes::from(source.to_string()));
        });
    });

    group.bench_function("bytes_from_static", |b| {
        b.iter(|| {
            let _b = black_box(Bytes::from_static(b"test_string_value"));
        });
    });

    group.finish();
}

// Benchmark: HashMap pre-allocation impact
fn bench_hashmap_allocation(c: &mut Criterion) {
    let mut group = c.benchmark_group("hashmap_allocation");

    let entries = vec![
        ("key1", "value1"),
        ("key2", "value2"),
        ("key3", "value3"),
        ("key4", "value4"),
        ("key5", "value5"),
    ];

    group.bench_function("without_capacity", |b| {
        b.iter(|| {
            let mut map = HashMap::new();
            for (k, v) in &entries {
                map.insert(k.to_string(), v.to_string());
            }
            black_box(map);
        });
    });

    group.bench_function("with_capacity", |b| {
        b.iter(|| {
            let mut map = HashMap::with_capacity(entries.len());
            for (k, v) in &entries {
                map.insert(k.to_string(), v.to_string());
            }
            black_box(map);
        });
    });

    group.finish();
}

// Benchmark: UTF-8 validation cost (happens in event_loop.rs when converting to Lua strings)
fn bench_utf8_validation(c: &mut Criterion) {
    let mut group = c.benchmark_group("utf8_validation");

    let ascii_bytes = Bytes::from("simple ascii text");
    group.bench_function("ascii_string", |b| {
        b.iter(|| {
            let _s = black_box(std::str::from_utf8(&ascii_bytes).unwrap());
        });
    });

    let unicode_bytes = Bytes::from("Unicode: こんにちは 🚀");
    group.bench_function("unicode_string", |b| {
        b.iter(|| {
            let _s = black_box(std::str::from_utf8(&unicode_bytes).unwrap());
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_clone_headers,
    bench_clone_query,
    bench_clone_params,
    bench_full_context_clone,
    bench_smallvec_threshold,
    bench_bytes_clone,
    bench_string_allocations,
    bench_hashmap_allocation,
    bench_utf8_validation
);

criterion_main!(benches);
