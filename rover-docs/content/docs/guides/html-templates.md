---
weight: 4
title: HTML Templates
---

Rover ships a tiny HTML templating helper via `rover.html`.

## Basic Usage

```lua
local render = rover.html({ title = "Home" })
local html = render([[<h1>{{ title }}</h1>]])
```

`rover.html(data)` returns a builder. Call it with a template string to render.

## HTTP Responses

The server HTML response builder uses the same templating:

```lua
function api.home.get()
  return api.html({ title = "Home" }) [[<h1>{{ title }}</h1>]]
end
```

## Expressions

Use `{{ ... }}` to evaluate Lua expressions:

```lua
local render = rover.html({ user = { name = "Zoe" } })
render([[<p>{{ user.name }}</p>]])
```

## Control Flow

Use `{{ if ... then }}`, `{{ elseif ... then }}`, `{{ else }}`, `{{ end }}` and `{{ for ... do }}`:

```lua
local render = rover.html({ items = { "a", "b" } })
render([[
  <ul>
    {{ for i, item in ipairs(items) do }}
      <li>{{ item }}</li>
    {{ end }}
  </ul>
]])
```

## Components

`rover.html` is a table, so you can attach helper functions and call them in templates:

```lua
function rover.html.card(props)
  return rover.html(props) [=[
    <div class="card">
      <h2>{{ title }}</h2>
      <div class="card-content">{{ content }}</div>
    </div>
  ]=]
end

local render = rover.html({})
render("{{ card { title = \"Hello\", content = \"World\" } }}")
```
