# WebSocket Implementation Plan

## Architecture Overview

Single-threaded mio event loop, no external WebSocket crate. Custom zero-copy frame parser/builder. All WS connections live in the same `Slab<Connection>` as HTTP. A central `WsManager` owns endpoints, subscriptions, and frame pools. Standard RFC 6455 protocol -- any browser/mobile/CLI client can connect.

**Two new dependency crates only**: `sha1` (handshake) and `base64` (handshake). Both called once per upgrade, not on the hot path.

---

## Phase 1: WebSocket Protocol Layer

### Step 1.1 -- Add Dependency Crates

**File**: `rover-server/Cargo.toml`

Add `sha1 = "0.10"` and `base64 = "0.22"` for the RFC 6455 handshake (SHA-1 + Base64 of `Sec-WebSocket-Key`). Called once per connection upgrade, not performance-critical.

---

### Step 1.2 -- Frame Parser & Builder

**File**: `rover-server/src/ws_frame.rs` (new)

**Types**:
```rust
#[repr(u8)]
pub enum WsOpcode {
    Continuation = 0x0,
    Text         = 0x1,
    Binary       = 0x2,
    Close        = 0x8,
    Ping         = 0x9,
    Pong         = 0xA,
}

/// Zero-copy frame header -- offsets into the connection's read_buf.
pub struct WsFrameHeader {
    pub fin: bool,
    pub opcode: WsOpcode,
    pub masked: bool,
    pub mask: [u8; 4],
    pub payload_offset: usize,  // into read_buf
    pub payload_len: usize,
    pub total_frame_len: usize, // header + mask + payload
}
```

**Functions**:
| Function | Purpose | Allocations |
|----------|---------|-------------|
| `try_parse_frame(buf: &[u8]) -> Option<WsFrameHeader>` | Parse one complete frame from buffer | **0** -- pure offset arithmetic |
| `unmask_payload_in_place(buf: &mut [u8], mask: [u8; 4])` | XOR unmask in-place, 4-byte unrolled loop | **0** |
| `write_frame(buf: &mut Vec<u8>, opcode, payload: &[u8])` | Build server->client frame (no mask, per RFC) | **0** -- writes into caller's pooled buf |
| `write_close_frame(buf: &mut Vec<u8>, code: u16, reason: &str)` | Close frame with status | **0** |
| `write_pong_frame(buf: &mut Vec<u8>, ping_payload: &[u8])` | Echo ping payload | **0** |

**Performance notes**:
- Server frames skip masking (RFC 6455 sec 5.1), saving 4 bytes + XOR pass.
- 4-byte XOR unrolled loop auto-vectorizes under `-O3` (LLVM).
- Frame layout: 2-byte header (payload<=125), 4-byte (<=65535), 10-byte (>65535).

---

### Step 1.3 -- Handshake Module

**File**: `rover-server/src/ws_handshake.rs` (new)

**Functions**:
| Function | Purpose |
|----------|---------|
| `validate_upgrade_request(conn: &Connection) -> Result<String, HandshakeError>` | Check Upgrade/Connection/Version/Key headers using existing offset-based header access. Returns the `Sec-WebSocket-Key`. |
| `compute_accept_key(client_key: &str) -> String` | SHA-1(key + magic GUID) -> Base64 |
| `build_upgrade_response(accept_key: &str, buf: &mut Vec<u8>)` | Write `101 Switching Protocols` into pooled buf (~130 bytes) |

**Error type**:
```rust
pub enum HandshakeError {
    MissingUpgradeHeader,
    MissingConnectionHeader,
    MissingKey,
    UnsupportedVersion,
    InvalidKey,
}
```

**Depends on**: Step 1.1 (sha1, base64 crates).

---

## Phase 2: Connection & Subscription Manager

### Step 2.1 -- Extend Connection for WebSocket

**File**: `rover-server/src/connection.rs`

**ConnectionState additions**:
```rust
pub enum ConnectionState {
    Reading,
    Writing,
    WsActive,   // Upgraded, bidirectional WS frames
    WsClosed,   // Close handshake initiated
    Closed,
}
```

**New WS-specific data** (only allocated after upgrade):
```rust
pub struct WsConnectionData {
    pub endpoint_idx: u16,
    pub state_key: Option<RegistryKey>,           // Lua state from join()
    pub write_queue: VecDeque<Bytes>,              // Pre-built frames, ref-counted
    pub write_pos: usize,                          // Position in front frame
    pub fragment_buf: Option<Vec<u8>>,             // Multi-frame message assembly
    pub subscriptions: SmallVec<[u16; 4]>,         // Topic indices, inline <=4
    pub close_sent: bool,
}
```

**Rationale**: `Option<WsConnectionData>` keeps HTTP connections slim (1 pointer = `None`). `VecDeque<Bytes>` for write queue because `Bytes::clone()` is O(1) refcount bump -- broadcast frames share one allocation across all recipients.

**New methods on Connection**:
```
is_websocket() -> bool
try_read_ws_frame() -> io::Result<Option<WsFrameHeader>>
try_write_ws() -> io::Result<bool>        // drain write_queue
queue_ws_frame(frame: Bytes)
upgrade_to_ws(endpoint_idx: u16)          // init WsConnectionData, clear HTTP state
ws_reset_read(consumed: usize)            // BytesMut::advance after frame consumed
```

Add `pub pending_ws_upgrade: Option<u16>` for tracking in-flight 101 responses.

**Depends on**: Step 1.2 (frame parsing).

---

### Step 2.2 -- WsManager

**File**: `rover-server/src/ws_manager.rs` (new)

```rust
pub struct WsEndpointConfig {
    pub join_handler: Option<RegistryKey>,
    pub leave_handler: Option<RegistryKey>,
    pub event_handlers: HashMap<String, RegistryKey>,  // O(1) dispatch
    pub ws_table_key: RegistryKey,                      // runtime ws.send context
}

struct TopicState {
    name: String,
    members: Vec<usize>,    // conn_idx list
}

pub struct WsManager {
    endpoints: Vec<WsEndpointConfig>,

    // Topic pub/sub
    topic_index: HashMap<String, u16>,   // name -> idx, O(1) lookup
    topics: Vec<TopicState>,

    // Per-endpoint connection tracking (avoid full slab scan for :all)
    endpoint_connections: Vec<Vec<usize>>,  // endpoint_idx -> [conn_idx]

    // Frame buffer pool
    frame_bufs: Vec<Vec<u8>>,  // pre-allocated, 64x 256 bytes

    // Per-handler-call context (safe: single-threaded, non-preemptive)
    pub current_conn_idx: usize,
    pub current_endpoint_idx: usize,
}
```

**Key methods**:
| Method | Purpose | Complexity |
|--------|---------|-----------|
| `register_endpoint(config) -> u16` | Add endpoint at startup | O(1) |
| `subscribe(conn_idx, topic) -> u16` | Subscribe connection to topic | O(1) amortized |
| `unsubscribe_all(conn_idx, connections)` | Remove from all topics on disconnect | O(T) where T = subscribed topics |
| `get_topic_members(topic) -> &[usize]` | Connection list for `:to(topic)` | O(1) |
| `get_endpoint_connections(idx) -> &[usize]` | All connections for `:all` | O(1) |
| `get_frame_buf() / return_frame_buf()` | Frame buffer pool | O(1) |
| `set_context(conn_idx, endpoint_idx)` | Set per-call context before handler | O(1) |

**Performance notes**:
- `endpoint_connections` side-vector avoids scanning full slab for `:all` broadcasts.
- Topic unsubscribe uses swap-remove: O(1) per topic.
- SmallVec inline for <=4 subscriptions per connection (no heap for typical use).

**Depends on**: Step 2.1.

---

## Phase 3: Lua DSL Bindings

### Step 3.1 -- WS Lua Table Factory

**File**: `rover-server/src/ws_lua.rs` (new)

Creates the `ws` table passed to `function api.chat.ws(ws)`.

**Table structure**:
```
ws (table with metatable)
  __ws_join          = nil (captured by __newindex when user assigns ws.join)
  __ws_leave         = nil (captured by __newindex when user assigns ws.leave)
  listen (table)
    __ws_handlers    = {} (event_name -> handler function)
    metatable:
      __newindex     -> captures: function ws.listen.chat(msg, ctx, state)
      __call         -> runtime: ws.listen("room:lobby") subscribes to topic
  send (table)
    metatable:
      __index        -> returns SendEventBuilder for any event name
  error(code, msg)   -> reject connection during join
  metatable:
    __newindex       -> captures ws.join and ws.leave assignments
```

**`ws.listen` metamethods**:
- `__newindex(self, key, fn)`: Stores `__ws_handlers[key] = fn`. Called at setup time.
- `__call(self, topic)`: At runtime, reads `WsManager.current_conn_idx` from Lua app_data, calls `ws_manager.subscribe(conn_idx, topic)`.

**`ws.send` metamethod**:
- `__index(self, event_name)`: Returns a **SendEventBuilder** table:

**SendEventBuilder** (returned by `ws.send.<event>`):
- `__call(self, data_table)`:
  - **With table arg** (`ws.send.ack { success = true }`): Reply to sender only.
  - **Without arg** (`ws.send.chat()`): Returns **TargetSelector** table.

**TargetSelector** (returned by `ws.send.<event>()`):
```
all(self, data)              -> broadcast to all endpoint connections
except(self, data)           -> broadcast except current_conn_idx
to(self, topic) -> fn(data)  -> send to topic subscribers
to_subscriptions(self, data) -> send to all topics current conn subscribes to
```

**Depends on**: Step 2.2 (WsManager).

---

### Step 3.2 -- Send Helpers

**File**: `rover-server/src/ws_send.rs` (new)

Separates serialization and broadcast logic from Lua binding boilerplate.

**Key functions**:
| Function | Purpose |
|----------|---------|
| `send_to_connection(conn, frame: Bytes, registry)` | Queue frame + register WRITABLE |
| `broadcast_frame(connections, targets: &[usize], frame: Bytes, except: Option<usize>, registry)` | Clone Bytes to each target (O(1) per clone) |
| `serialize_event_json(lua, event_name, data: &Table, buf)` | JSON with injected `"type"` field |
| `build_event_frame(json_body: &[u8], frame_buf)` | Wrap JSON in WS text frame |

**Serialization strategy** (hot path optimization):
1. Write `{"type":"<event_name>",` to pooled buf.
2. Serialize table key-value pairs directly (reuse `to_json.rs` inner loop, skip opening `{`).
3. Write closing `}`.
- Avoids creating a wrapper Lua table with `type` field.
- Avoids double-serialization.

**Broadcast allocation budget**:
- 1x JSON serialization (into pooled `Vec<u8>`)
- 1x frame build (into same or another pooled `Vec<u8>`)
- 1x `Bytes::from(vec)` (takes ownership, no copy)
- Nx `Bytes::clone()` = N atomic refcount bumps (~1ns each)
- **Total for broadcast to 10,000 clients**: 1 alloc + 10,000 refcount bumps (~10us)

**Depends on**: Step 3.1, Step 1.2.

---

## Phase 4: Event Loop Integration

### Step 4.1 -- Add WsManager to EventLoop

**File**: `rover-server/src/event_loop.rs`

```rust
pub struct EventLoop {
    // ... existing fields ...
    ws_manager: Rc<RefCell<WsManager>>,  // shared with Lua app_data
}
```

In `EventLoop::new()`:
1. Create `WsManager::new()`.
2. For each `WsRoute`: call `ws_manager.register_endpoint(config)`.
3. Build `FastRouter` with both HTTP routes and WS routes.
4. Wrap in `Rc<RefCell<>>`, store locally and in `lua.set_app_data(...)`.

---

### Step 4.2 -- HTTP Upgrade Flow

**File**: `rover-server/src/event_loop.rs`

**Changes to `start_request_coroutine`**:

After HTTP parsing, before route matching:
1. Check if `Upgrade: websocket` header is present (scan `header_offsets`).
2. If yes: match against `FastRouter::match_ws_route(path)`.
3. If WS match found: call `handle_ws_upgrade(conn_idx, endpoint_idx, params)`.
4. If no match: 404.
5. If no Upgrade header: existing HTTP flow.

**`handle_ws_upgrade(conn_idx, endpoint_idx, params)`**:
1. `validate_upgrade_request(&conn)` -- check all required headers.
2. On failure: send 400/426 HTTP error, return.
3. `compute_accept_key(client_key)`.
4. `build_upgrade_response(accept_key, pooled_buf)`.
5. Write 101 to `conn.write_buf`, set `conn.pending_ws_upgrade = Some(endpoint_idx)`.
6. Set state to `Writing`, register WRITABLE.

**After 101 fully written** (in `handle_connection` write completion):
1. Check `conn.pending_ws_upgrade`.
2. If Some: `conn.upgrade_to_ws(endpoint_idx)`, state -> `WsActive`.
3. Create `RequestContext` from pool with HTTP headers/query/params.
4. Call `ws.join(ctx)` via coroutine.
5. Store returned Lua value as connection state via `RegistryKey`.
6. Register READABLE.

---

### Step 4.3 -- WS Read/Write in Event Loop

**File**: `rover-server/src/event_loop.rs`

**New branch in `handle_connection`**:
```rust
if conn.is_websocket() {
    return self.handle_ws_event(conn_idx, event);
}
```

**`handle_ws_event` read path** (`event.is_readable()`):
1. Read from socket into `read_buf`.
2. Loop: `try_parse_frame(read_buf)` for each complete frame:
   - **Text (0x1)**: Unmask in-place. Parse JSON via `direct_json_parser`. Extract `"type"` field -> look up `event_handlers[type]`. Remove `"type"` from msg table. Call handler as `(msg, ctx, state)` coroutine. Handle yields via existing `PendingCoroutine` mechanism.
   - **Ping (0x9)**: Queue pong frame (echo payload).
   - **Pong (0xA)**: Ignore.
   - **Close (0x8)**: Queue close frame response if not sent. Call `ws.leave(state)`. Transition to `WsClosed`.
   - **Continuation (0x0)**: Append to `fragment_buf`. On FIN=1, process assembled message.
   - **Binary (0x2)**: Treat as text (JSON) for now.
3. `BytesMut::advance(consumed)` after each frame.
4. EOF (0 bytes read): disconnect -> call `ws.leave(state)`, cleanup.

**Write path** (`event.is_writable()`):
1. `conn.try_write_ws()` drains `write_queue`.
2. If empty: register READABLE only (avoid busy-spin).
3. If data remains: keep WRITABLE.

**Performance**: All available frames processed per readable event, amortizing mio poll cost.

---

### Step 4.4 -- Disconnect & Cleanup

**File**: `rover-server/src/event_loop.rs`

**`handle_ws_disconnect(conn_idx)`**:
1. Set WsManager context.
2. Retrieve state `RegistryKey`.
3. Call `leave_handler(state)` if present (may trigger `ws.send.*` broadcasts).
4. `ws_manager.unsubscribe_all(conn_idx)`.
5. Remove state from Lua registry.
6. Remove from `endpoint_connections`.
7. Deregister from mio poll.
8. Remove from `Slab<Connection>`.

**Timeouts**: WS connections are long-lived; skip the existing HTTP coroutine timeout for WS. Future: add WS idle ping/pong interval.

---

## Phase 5: Route Extraction

### Step 5.1 -- Detect `ws` Endpoints

**File**: `rover-core/src/server.rs`

**Changes to `extract_recursive`**:

In the `(Value::String, Value::Function)` match arm, **before** `HttpMethod::from_str`:
```rust
if key_string == "ws" {
    let ws_route = extract_ws_endpoint(lua, func, path, param_names)?;
    ws_routes.push(ws_route);
    continue;
}
```

**`extract_ws_endpoint(lua, setup_fn, path, param_names) -> WsRoute`**:
1. Create ws table via `ws_lua::create_ws_table(lua)`.
2. Call `setup_fn(ws_table)` -- user's setup code runs, assigns handlers.
3. Extract from ws table: join handler, leave handler, event handlers HashMap.
4. Store each as `RegistryKey`.
5. Build `WsEndpointConfig` and `WsRoute`.

**Thread `lua: &Lua`** through `extract_recursive` (currently only takes the table).
Add `ws_routes: &mut Vec<WsRoute>` accumulator parameter.

---

### Step 5.2 -- WS Routes in FastRouter

**File**: `rover-server/src/fast_router.rs`

Add separate WS routing:
```rust
pub struct FastRouter {
    // ... existing HTTP fields ...
    ws_router: Router<u16>,                // path pattern -> endpoint_idx
    ws_static: HashMap<u64, u16>,          // hash(path) -> endpoint_idx
}
```

New method: `match_ws_route(path) -> Option<(u16, Vec<(Bytes, Bytes)>)>`.

**Request flow**:
1. Has `Upgrade: websocket`? -> try `match_ws_route` first.
2. No upgrade header? -> try `match_route` (existing HTTP path).

---

### Step 5.3 -- Wire at Startup

**Files changed**: `rover-server/src/lib.rs`, `rover-server/src/http_server.rs`, `rover-core/src/server.rs`

- `RouteTable` gains `ws_routes: Vec<WsRoute>`.
- `rover_server::run()` accepts ws_routes.
- `EventLoop::new()` registers endpoints in WsManager, builds FastRouter with both.

---

## Module Registration

**File**: `rover-server/src/lib.rs`

```rust
mod ws_frame;
mod ws_handshake;
pub mod ws_manager;
pub mod ws_lua;
pub mod ws_send;
```

---

## Memory Allocation Budget Per Operation

| Operation | Heap Allocations | Notes |
|-----------|-----------------|-------|
| Frame parse | **0** | Offset arithmetic on read_buf |
| Frame unmask | **0** | XOR in-place |
| JSON message parse | ~N Lua tables | Same as HTTP body, streaming deserializer |
| Event dispatch | **0** | HashMap lookup |
| Send (reply to sender) | **1** pooled Vec | JSON + frame build into pooled buf |
| Send (broadcast to N) | **1** pooled Vec + N refcount bumps | Bytes::clone = O(1) |
| Subscribe | **0-1** | SmallVec inline <=4; HashMap entry if new topic |
| Connection upgrade | **1** Box<WsConnectionData> | Created once per WS connection lifetime |
| Connection close | **0** | Cleanup frees existing allocations |

---

## Implementation Order (Dependency Graph)

```
1.1 (Cargo deps)
  |
  v
1.2 (ws_frame.rs) -------> 1.3 (ws_handshake.rs)
  |                              |
  v                              |
2.1 (Connection changes)         |
  |                              |
  v                              |
2.2 (ws_manager.rs)              |
  |                              |
  v                              |
3.1 (ws_lua.rs)                  |
  |                              |
  v                              |
3.2 (ws_send.rs)                 |
  |                              |
  v                              |
5.1 (route extraction)           |
  |                              |
  v                              |
5.2 (FastRouter WS) <-----------+
  |
  v
4.1 (EventLoop + WsManager)
  |
  v
4.2 (upgrade flow)
  |
  v
4.3 (WS read/write)
  |
  v
4.4 (disconnect/cleanup)
  |
  v
5.3 (wire startup)
```

---

## Client Compatibility

Standard RFC 6455 -- any WebSocket client connects:
- **Browser**: `new WebSocket("ws://host/chat/ws?user_id=alice")`
- **Mobile**: OkHttp (Android), URLSessionWebSocketTask (iOS)
- **CLI**: websocat, wscat, or any WS library
- **Other Rover clients**: future `ws.new(...)` Lua client

Message envelope convention: `{"type":"event_name", ...}` (JSON text frames). Clients unaware of this convention can still use the `ws.listen.message` catch-all handler for raw messages.
