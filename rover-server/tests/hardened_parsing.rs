use mlua::{Lua, Value};
use rover_server::Bytes;
use rover_server::direct_json_parser::json_bytes_ref_to_lua_direct;

fn parse_json(bytes: &[u8]) -> mlua::Result<Value> {
    let lua = Lua::new();
    let bytes = Bytes::copy_from_slice(bytes);
    json_bytes_ref_to_lua_direct(&lua, &bytes)
}

#[test]
fn should_reject_trailing_non_whitespace_bytes() {
    let err = parse_json(br#"{"ok":true}garbage"#).expect_err("trailing bytes should be rejected");
    assert!(err.to_string().contains("JSON parsing failed"));
}

#[test]
fn should_reject_multiple_root_values() {
    let err = parse_json(br#"{"ok":true}{}"#).expect_err("multiple roots should be rejected");
    assert!(err.to_string().contains("JSON parsing failed"));
}

#[test]
fn should_accept_trailing_whitespace() {
    let value = parse_json(b"{\"ok\":true} \n\t").expect("trailing whitespace should parse");
    assert!(matches!(value, Value::Table(_)));
}

#[test]
fn should_reject_incomplete_json_documents() {
    let err = parse_json(br#"{"ok":true"#).expect_err("incomplete document should fail");
    assert!(err.to_string().contains("JSON parsing failed"));
}
