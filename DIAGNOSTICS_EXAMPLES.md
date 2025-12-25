# Rover Check - Diagnostic Examples

This document shows example outputs from the `rover check` command and the automatic pre-execution analyzer.

## Table of Contents
1. [Basic Usage](#basic-usage)
2. [Verbose Mode](#verbose-mode)
3. [Error Detection](#error-detection)
4. [Pre-Execution Check](#pre-execution-check)
5. [JSON Output](#json-output)

---

## Basic Usage

### Command
```bash
rover check examples/rest_api_basic.lua
```

### Output - Success
```

Analyzing Rover code...
============================================================

✓ No errors found!
```

---

## Verbose Mode

### Command
```bash
rover check examples/rest_api_basic.lua --verbose
```

### Output - Detailed Analysis
```

Analyzing Rover code...
============================================================

✓ No errors found!

Analysis Summary:
------------------------------------------------------------
  Server: exported ✓
  Routes: 4

  Route Details:
    GET /hello (1 responses)
    GET /hello/{id} (1 responses)
      → param: id ✓
    GET /users/{id}/posts/{postId} (1 responses)
      → param: id ✓
      → param: postId ✓
    GET /greet/{name} (1 responses)
      → param: name ✓
  Functions: 4
```

### Example with Query Params and Headers
```bash
rover check examples/context_requests.lua --verbose
```

```

Analyzing Rover code...
============================================================

✓ No errors found!

Analysis Summary:
------------------------------------------------------------
  Server: exported ✓
  Routes: 2

  Route Details:
    GET /echo (1 responses)
      → query: page
      → query: limit
      → header: user-agent
    POST /echo (1 responses)
      → header: content-type
  Functions: 2
```

---

## Error Detection

### Example 1: Non-existent Parameter

#### File: `app_error.lua`
```lua
local api = rover.server {}

function api.hello.p_id.get(ctx)
    return api.json {
        message = "Hello " .. ctx:params().nonexistent  -- Error: param doesn't exist
    }
end

return api
```

#### Command
```bash
rover check app_error.lua
```

#### Output
```

Analyzing Rover code...
============================================================

✗ error found:

error: app_error.lua:5:31
  Accessing non-existent path param 'nonexistent'. Available params: ["id"]
  help: Check that you're accessing the correct parameter name. Available params are defined in your route path.

```

**Exit Code:** 1

---

### Example 2: Multiple Errors

When multiple errors are detected:

```

Analyzing Rover code...
============================================================

✗ 3 errors found:

error: app.lua:12:20
  Accessing non-existent path param 'userId'. Available params: ["id"]
  help: Check that you're accessing the correct parameter name. Available params are defined in your route path.

error: app.lua:18:15
  Accessing non-existent query param 'search'
  help: Ensure the variable or function is defined before use.

error: app.lua:25:9
  Failed to parse response
  help: Verify your route definition follows the pattern: api.path.method(ctx)

```

**Exit Code:** 1

---

## Pre-Execution Check

When running `rover app.lua`, the checker runs automatically before executing the code.

### Success Case

#### Command
```bash
rover examples/rest_api_basic.lua
```

#### Output
```
✓ Code analysis passed

[Server starts normally...]
```

---

### Error Case

#### Command
```bash
rover app_with_errors.lua
```

#### Output
```
────────────────────────────────────────────────────────────
Rover Check: found 2 issue(s)
────────────────────────────────────────────────────────────
  ✗ app_with_errors.lua:5:31 - Accessing non-existent path param 'nonexistent'. Available params: ["id"]
    → Check that you're accessing the correct parameter name. Available params are defined in your route path.
  ✗ app_with_errors.lua:12:15 - Variable 'undefined_var' not found
    → Ensure the variable or function is defined before use.
────────────────────────────────────────────────────────────

[Server starts anyway - issues shown as warnings]
```

**Note:** The pre-execution check shows warnings but doesn't prevent execution, allowing you to see runtime behavior even with static analysis warnings.

---

## JSON Output

For CI/CD integration and tooling.

### Command
```bash
rover check app_error.lua --format json
```

### Output
```json
{
  "file": "app_error.lua",
  "errors": [
    {
      "function": "api.hello.p_id.get",
      "message": "Accessing non-existent path param 'nonexistent'. Available params: [\"id\"]",
      "range": {
        "start": {
          "line": 4,
          "column": 30
        },
        "end": {
          "line": 4,
          "column": 42
        }
      }
    }
  ],
  "error_count": 1,
  "server_found": true,
  "routes_count": 1,
  "functions_count": 1
}
```

**Exit Code:** 1

---

## Use Cases

### 1. Development Workflow
```bash
# Quick check before running
rover check app.lua && rover app.lua
```

### 2. CI/CD Pipeline
```bash
# Fail build on errors
rover check src/*.lua --format json | jq '.error_count'
```

### 3. Pre-commit Hook
```bash
#!/bin/bash
for file in $(git diff --cached --name-only --diff-filter=ACM | grep '\.lua$'); do
    rover check "$file" || exit 1
done
```

### 4. LSP Debugging
The verbose output helps understand what the analyzer extracts:
```bash
rover check app.lua --verbose
```
Shows exactly what routes, params, headers, and functions the LSP will see.

---

## What Gets Analyzed

- ✅ **Server Definition**: Checks if `rover.server` is defined and exported
- ✅ **Routes**: Validates HTTP method handlers (GET, POST, PUT, DELETE, etc.)
- ✅ **Path Parameters**: Tracks `p_name` patterns and validates usage
- ✅ **Query Parameters**: Detects `ctx:query()` calls
- ✅ **Headers**: Tracks `ctx:headers()` usage
- ✅ **Request Body**: Analyzes body schemas and validation guards
- ✅ **Responses**: Captures response structures and status codes
- ✅ **Functions**: Tracks all function definitions

---

## Error Types & Suggestions

| Error Pattern | Suggestion |
|--------------|------------|
| Non-existent param | Check that you're accessing the correct parameter name |
| Variable not found | Ensure the variable or function is defined before use |
| Guard validation error | Review your guard definition syntax |
| Route parsing error | Verify your route definition follows the pattern |
| Validation schema error | Check your validation schema for proper structure |

---

## Color Coding

When viewed in a terminal with color support:

- ✅ **Green**: Success, valid items
- 🔴 **Red**: Errors
- 🟡 **Yellow**: Warnings, line numbers
- 🔵 **Cyan**: Info, suggestions, metadata
- ⚪ **White**: Main content, error messages
- 🌫️ **Dimmed**: Secondary info

---

## Tips

1. **Use verbose mode during development** to understand what the analyzer sees
2. **Use JSON output in CI/CD** for programmatic error handling
3. **The pre-execution check runs automatically** when you use `rover app.lua`
4. **Exit codes matter**: 0 = success, 1 = errors found
5. **Suggestions are context-aware** based on the error type
