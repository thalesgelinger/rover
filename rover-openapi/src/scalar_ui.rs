use serde_json::Value;

pub fn scalar_html(spec: &Value) -> String {
    let spec_json = serde_json::to_string(spec).unwrap_or_default();

    format!(
        r#"<!doctype html>
<html>
  <head>
    <title>API Documentation</title>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
  </head>
  <body>
    <div id="app"></div>
    <script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference"></script>
    <script>
      const configuration = {{
        content: {spec_json}
      }}

      Scalar.createApiReference(
        document.getElementById('app'),
        configuration
      )
    </script>
  </body>
</html>"#
    )
}
