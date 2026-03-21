-- Foundation TLS + lifecycle example
--
-- Replace cert paths before running in HTTPS mode.
--
-- Run:
--   cargo run -p rover_cli -- run examples/foundation_tls_lifecycle.lua

local api = rover.server {
  host = "0.0.0.0",
  port = 8443,
  strict_mode = true,
  allow_public_bind = true,
  security_headers = true,
  drain_timeout_secs = 30,
  tls = {
    cert_file = "./certs/dev-cert.pem",
    key_file = "./certs/dev-key.pem",
    reload_interval_secs = 300,
  },
}

api.on_start = function()
  print("starting https server")
end

api.on_ready = function()
  print("ready: https://localhost:8443")
end

api.on_shutdown = function()
  print("shutdown requested, draining connections")
end

-- Note: Use built-in /healthz and /readyz probes instead of custom endpoints.
-- These are automatically provided by rover.server() and should not be redefined.

return api
