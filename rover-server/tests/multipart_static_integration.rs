use bytes::Bytes;
use mlua::Lua;
use rover_server::http_task::BodyValue;
use rover_server::serve_static_file;
use std::fs;
use tempfile::tempdir;

#[test]
fn should_parse_real_multipart_upload_flow_via_body_userdata() {
    let lua = Lua::new();

    let boundary = "----RoverBoundaryXYZ";
    let content_type = format!("multipart/form-data; boundary={}", boundary);
    let body = format!(
        "--{boundary}\r\n\
Content-Disposition: form-data; name=\"username\"\r\n\r\n\
alice\r\n\
--{boundary}\r\n\
Content-Disposition: form-data; name=\"avatar\"; filename=\"avatar.png\"\r\n\
Content-Type: image/png\r\n\r\n\
PNGDATA\r\n\
--{boundary}--\r\n"
    );

    let body_value = BodyValue::new(Bytes::from(body), Some(content_type));
    let body_ud = lua
        .create_userdata(body_value)
        .expect("create BodyValue userdata");
    lua.globals().set("body", body_ud).expect("set global body");

    let (username, file_name, file_size, file_type, file_data, multipart_file_name): (
        String,
        String,
        usize,
        String,
        String,
        String,
    ) = lua
        .load(
            r#"
            local form = body:form()
            local file = body:file("avatar")
            local all = body:multipart()

            return form.username,
              file.name,
              file.size,
              file.type,
              tostring(file.data),
              all.files.avatar[1].name
        "#,
        )
        .eval()
        .expect("evaluate multipart script");

    assert_eq!(username, "alice");
    assert_eq!(file_name, "avatar.png");
    assert_eq!(multipart_file_name, "avatar.png");
    assert_eq!(file_size, 7);
    assert_eq!(file_type, "image/png");
    assert_eq!(file_data, "PNGDATA");
}

#[test]
fn should_serve_static_asset_path_with_route_style_leading_slash() {
    let dir = tempdir().expect("create temp dir");
    let assets_dir = dir.path().join("assets");
    fs::create_dir_all(&assets_dir).expect("create assets dir");

    let script_path = assets_dir.join("app.js");
    fs::write(&script_path, b"console.log('ok');").expect("write app.js");

    let response = serve_static_file(dir.path(), "/assets/app.js", None, None);

    assert_eq!(response.status, 200);
    assert_eq!(response.content_type, "application/javascript");
    assert_eq!(response.body, Bytes::from_static(b"console.log('ok');"));

    let headers = response.headers.expect("cache headers should exist");
    assert!(headers.contains_key("Cache-Control"));
    assert!(headers.contains_key("ETag"));
}
