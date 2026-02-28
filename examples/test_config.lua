-- Test config file for rover.config.load() example
return {
  app_name = "Rover Demo",
  version = "1.0.0",
  features = {
    "auth",
    "database",
    "websocket",
  },
  database = {
    host = "localhost",
    port = 5432,
    name = "myapp",
  },
  settings = {
    max_connections = 100,
    timeout_seconds = 30,
  },
}
