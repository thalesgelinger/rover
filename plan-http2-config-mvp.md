# HTTP/2 Configuration MVP - Implementation Plan

## Context
Task: Complete unchecked acceptance criterion from US-007:
"Configuration exposes a minimal production-safe MVP control set for HTTP/2 rollout: enable/disable switch, ALPN fallback safety, and only implementation-supported extra limits."

Current state:
- ✅ Enable/disable switch exists (`http2: bool` in ServerConfig)
- ❌ ALPN fallback safety configuration missing
- ❌ Implementation-supported extra limits missing

## Analysis

### What exists now:
1. `http2: bool` field in ServerConfig (line 412 lib.rs)
2. Parsed from Lua config (lines 1191-1195)
3. Default: true
4. Tests for enabled/disabled states

### What's needed:

#### 1. ALPN Fallback Safety
Since the server uses TLS (rustls), we need:
- Configure ALPN protocol list based on `http2` flag
- When http2=true: advertise ["h2", "http/1.1"]
- When http2=false: advertise ["http/1.1"]
- Ensure safe fallback to HTTP/1.1 if negotiation fails

#### 2. Implementation-Supported Extra Limits
Per PRD line 33: "HTTP/2 MVP extra knobs default to none beyond explicit enable/disable unless the implementation naturally exposes one small safety control cleanly"

Since HTTP/2 isn't actually implemented yet (code only does HTTP/1.1), there are NO implementation-supported limits currently. The minimal safe approach is:
- Document that no extra limits are available in this MVP
- Configuration schema should not expose any tuning knobs beyond enable/disable
- Add validation that warns if http2=true but TLS is not configured

## Design

### Schema Changes

```rust
pub struct ServerConfig {
    // ... existing fields ...
    
    /// Enable HTTP/2 support (requires TLS). Default: true
    /// When enabled, ALPN will advertise "h2" and "http/1.1"
    /// When disabled, ALPN will advertise only "http/1.1"
    pub http2: bool,
    
    // No additional fields needed - PRD requires minimal config
}
```

### Validation Changes

Add startup validation (in `validate_startup` method):
- If `http2 == true` and `tls.is_none()`: warn or error (HTTP/2 requires TLS)
- Ensure ALPN configuration matches http2 flag

### ALPN Integration Points

Since rustls is in Cargo.toml, the TLS setup should:
1. Build ALPN list: `["h2", "http/1.1"]` if http2=true, else `["http/1.1"]`
2. Pass ALPN list to rustls ServerConfig
3. Safe fallback: rustls will negotiate highest common protocol

## Implementation Steps

1. **Add validation** in `ServerConfig::validate_startup()`:
   - Check if http2=true without TLS → warn/error
   
2. **Ensure ALPN config** in TLS setup:
   - Need to find where TLS is configured and add ALPN protocol list
   - This might be in event_loop.rs or a TLS module

3. **Add tests**:
   - ALPN list when http2 enabled
   - ALPN list when http2 disabled
   - Validation when http2=true without TLS

4. **Document** in code comments:
   - That no extra limits are exposed (per PRD requirements)

5. **Update existing tests** to verify ALPN fallback safety

## Unresolved Questions

1. Where is TLS server configuration created? Need to find the exact location to add ALPN protocol list.

2. Should http2=true without TLS be a warning or an error?
   - PRD line 26: "production-safe MVP"
   - Recommendation: Error in production/stict mode, warning otherwise

3. Does ALPN setup require changes to how rustls is configured, or is there already a place for ALPN?