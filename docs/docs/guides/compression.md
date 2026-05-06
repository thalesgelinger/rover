---
sidebar_position: 8
---

# Response Compression

Rover supports automatic response compression to reduce bandwidth and improve performance. The server negotiates compression algorithms with clients via the `Accept-Encoding` header and applies compression based on configurable rules.

## Quick Start

Compression is **enabled by default** with sensible defaults:

```lua
local api = rover.server {
    -- Compression is automatically enabled with defaults:
    -- - gzip and deflate algorithms
    -- - 1024 byte minimum size threshold
    -- - compressible content types only
}
```

## Configuration Options

Configure compression behavior with the `compress` table:

```lua
local api = rover.server {
    compress = {
        enabled = true,              -- Enable/disable compression (default: true)
        algorithms = { "gzip", "deflate" },  -- Supported algorithms (default: both)
        min_size = 1024,             -- Minimum response size in bytes (default: 1024)
        types = {                    -- Content types to compress (default: auto-detected)
            "application/json",
            "text/html",
            "text/plain"
        }
    }
}
```

### `compress.enabled`

- **Type**: `boolean`
- **Default**: `true`
- **Description**: Enable or disable response compression entirely

```lua
-- Disable compression
local api = rover.server {
    compress = {
        enabled = false
    }
}
```

### `compress.algorithms`

- **Type**: `array of strings`
- **Default**: `{ "gzip", "deflate" }`
- **Options**: `"gzip"`, `"deflate"`
- **Description**: Compression algorithms to support. Order determines preference when client accepts both.

```lua
-- Support only gzip
local api = rover.server {
    compress = {
        algorithms = { "gzip" }
    }
}

-- Prefer deflate over gzip
local api = rover.server {
    compress = {
        algorithms = { "deflate", "gzip" }
    }
}
```

**Note**: Rover only supports `gzip` and `deflate`. Other algorithms like `br` (Brotli) or `zstd` are not supported and will be ignored during negotiation.

### `compress.min_size`

- **Type**: `number`
- **Default**: `1024` (1 KB)
- **Description**: Minimum response body size in bytes to trigger compression. Smaller responses are not compressed to avoid overhead.

```lua
-- Only compress responses larger than 4 KB
local api = rover.server {
    compress = {
        min_size = 4096
    }
}
```

### `compress.types`

- **Type**: `array of strings`
- **Default**: `[]` (empty, uses auto-detection)
- **Description**: Explicit list of MIME types to compress. When empty, Rover automatically detects compressible types.

```lua
-- Explicitly define compressible types
local api = rover.server {
    compress = {
        types = {
            "application/json",
            "text/html",
            "text/css",
            "application/javascript"
        }
    }
}
```

## Behavior Notes

### Content-Type Detection

When `compress.types` is empty (default), Rover automatically compresses these content types:

- `text/*` (HTML, CSS, plain text, XML, etc.)
- `application/json`
- `application/javascript`
- `application/xml`
- `application/atom+xml`
- `application/vnd.api+json`

These types are **never compressed**:

- `text/event-stream` (SSE)
- Binary formats: images (`image/*`), videos (`video/*`), audio (`audio/*`)
- Already compressed formats: `application/gzip`, `application/zip`, etc.

### Algorithm Negotiation

Rover selects the compression algorithm based on the client's `Accept-Encoding` header:

1. Parses quality values (`q` parameters) from the header
2. Selects the highest-quality supported algorithm
3. Falls back to wildcard (`*`) matches if no explicit match
4. Returns uncompressed if no compatible algorithm found

Examples:

```
Accept-Encoding: gzip              → gzip
Accept-Encoding: deflate, gzip    → deflate (first preferred)
Accept-Encoding: gzip;q=0.5, deflate;q=0.9  → deflate (higher q)
Accept-Encoding: br, gzip         → gzip (br not supported)
Accept-Encoding: identity         → no compression
```

### Response Headers

When compression is applied:

- `Content-Encoding`: Set to the algorithm used (`gzip` or `deflate`)
- `Vary: Accept-Encoding`: Added to prevent cache collisions
- `Content-Length`: Updated to reflect compressed size
- `ETag`: Recalculated based on compressed content

### Streaming and SSE

Streaming responses (including Server-Sent Events) are **never compressed** even when enabled. This prevents buffering and ensures real-time delivery.

### Small Responses

Responses smaller than `min_size` are not compressed. This avoids compression overhead on small payloads where savings would be minimal or negative.

### Conditional Requests

Compressed responses work correctly with:

- `ETag`-based conditional requests (`If-None-Match`)
- `Last-Modified` validation (`If-Modified-Since`)
- Range requests (when supported by content type)

## Complete Example

```lua
local api = rover.server {
    host = "0.0.0.0",
    port = 8080,
    compress = {
        enabled = true,
        algorithms = { "gzip", "deflate" },
        min_size = 1024,
        types = {}  -- Use auto-detection
    }
}

-- Large JSON response will be compressed
function api.data.get(ctx)
    local data = {}
    for i = 1, 1000 do
        data[i] = { id = i, value = "some data " .. i }
    end
    return api.json(data)
end

-- Small response won't be compressed (< 1024 bytes)
function api.health.get(ctx)
    return api.json { status = "ok" }
end

-- SSE streaming never compressed
function api.events.get(ctx)
    return api.sse(function(writer)
        for i = 1, 10 do
            writer.send({ event = "tick", data = i })
            rover.time.sleep(1000)
        end
    end)
end

return api
```

## Testing Compression

Test compression behavior with curl:

```bash
# Request gzip compression
curl -H "Accept-Encoding: gzip" http://localhost:8080/data --compressed -v

# Request deflate compression  
curl -H "Accept-Encoding: deflate" http://localhost:8080/data --compressed -v

# No compression (identity)
curl -H "Accept-Encoding: identity" http://localhost:8080/data -v

# Check response headers
# Look for: Content-Encoding: gzip
#           Vary: Accept-Encoding
```

## Performance Considerations

- Compression adds CPU overhead on the server
- Use higher `min_size` values (e.g., 2048-4096) if CPU is constrained
- For high-traffic APIs, consider disabling compression on very small JSON responses
- Compression is most effective on repetitive text content (HTML, JSON arrays)

## Error Handling

Invalid compression configuration will fail server startup with clear error messages:

```lua
-- This will error: 'brotli' is not supported
local api = rover.server {
    compress = {
        algorithms = { "brotli" }
    }
}
-- Error: compress.algorithms contains unsupported value 'brotli'; 
--         supported values: 'gzip' and 'deflate'
```
