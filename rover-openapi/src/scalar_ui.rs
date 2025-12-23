use serde_json::Value;

pub fn scalar_html(spec: &Value) -> String {
    let spec_json = spec.to_string();

    format!(
        r#"<!DOCTYPE html>
<html>
  <head>
    <title>API Documentation</title>
    <meta charset="utf-8" />
    <meta
      name="viewport"
      content="width=device-width, initial-scale=1" />
    <style>
      body {{
        margin: 0;
        padding: 0;
      }}
    </style>
  </head>
  <body>
    <script
      id="api-reference"
      data-url="about:blank"
      src="https://cdn.jsdelivr.net/npm/@scalar/standalone@latest/dist/scalar.standalone.js"></script>
    <script>
      const spec = {spec_json};
      document.getElementById('api-reference').setAttribute('data-spec', JSON.stringify(spec));
    </script>
  </body>
</html>"#,
        spec_json = spec_json
    )
}
