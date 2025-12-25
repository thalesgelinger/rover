# rover-check

A command-line code analyzer and linter for Rover Lua applications. This tool helps you catch errors, validate your code structure, and understand your Rover application before running it.

## Features

- 🔍 **Static Analysis**: Parse and analyze Rover Lua code without executing it
- 🎨 **Colored Output**: Beautiful, easy-to-read error messages and diagnostics
- 💡 **Smart Suggestions**: Get helpful hints to fix common issues
- 📊 **Detailed Reports**: View comprehensive analysis of routes, params, and more
- 📋 **JSON Output**: Machine-readable output for CI/CD integration

## Installation

Build from the workspace root:

```bash
cargo build --package rover-check --release
```

The binary will be available at `target/release/rover-check`.

## Usage

### Basic Usage

Check a single Lua file:

```bash
rover-check your_app.lua
```

### Verbose Output

Get detailed analysis including routes, parameters, and function counts:

```bash
rover-check your_app.lua --verbose
```

### JSON Output

Get machine-readable output for integration with other tools:

```bash
rover-check your_app.lua --format json
```

## Example Output

### Success (with --verbose)

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

### Error Detection

```
Analyzing Rover code...
============================================================

✗ error found:

error: app.lua:5:31
  Accessing non-existent path param 'nonexistent'. Available params: ["id"]
  help: Check that you're accessing the correct parameter name. Available params are defined in your route path.
```

## What It Checks

rover-check analyzes your Rover code for:

- ✅ Server definition and exports
- ✅ Route structure and HTTP methods
- ✅ Path parameter usage and validation
- ✅ Query parameter detection
- ✅ Header usage tracking
- ✅ Request body schemas
- ✅ Response definitions
- ✅ Guard validations

## Exit Codes

- `0`: No errors found
- `1`: Errors detected in the code

## Integration

### CI/CD Pipeline

```bash
# Fail the build if errors are found
rover-check src/main.lua || exit 1
```

### Pre-commit Hook

```bash
#!/bin/bash
for file in $(git diff --cached --name-only --diff-filter=ACM | grep '\.lua$'); do
    rover-check "$file" || exit 1
done
```

## Development

The checker is built on top of `rover-parser`, which uses tree-sitter for Lua parsing and provides semantic analysis of Rover-specific patterns.

## License

Same as the main Rover project.
