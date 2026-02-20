local user_id = arg and arg[1] or "cli"
local auto_text = arg and arg[2] or nil
local auto_quit = arg and arg[3] == "quit" or false
local ws = rover.ws_client("ws://localhost:4242/chat")

local has_tui_runtime = false
if rover.task ~= nil then
  has_tui_runtime = pcall(function()
    rover.task(function()
      return nil
    end)
  end)
end

if not has_tui_runtime then
  function ws.join(ctx)
    print("connected as", user_id)
    print("type message + Enter, /quit to exit")
    ws.send.identify { user_id = user_id }

    if auto_text ~= nil and auto_text ~= "" then
      ws.send.chat { text = auto_text }
      if auto_quit then
        ws:close(1000, "auto quit")
      end
    end

    return { user_id = user_id }
  end

  function ws.listen.chat(msg, ctx, state)
    local who = tostring(msg.user_id or "anon")
    if who ~= user_id then
      print("[chat]", who .. ":", tostring(msg.text or ""))
    end
  end

  function ws.listen.user_joined(msg, ctx, state)
    local who = tostring(msg.user_id or "anon")
    if who ~= user_id then
      print("[join]", who)
    end
  end

  function ws.listen.user_left(msg, ctx, state)
    print("[left]", tostring(msg.user_id or "anon"))
  end

  function ws.error(err, ctx, state)
    print("ws error:", tostring(err.message or "unknown"))
  end

  function ws.leave(info, state)
    print("closed:", info.code, info.reason)
  end

  ws:connect()

  while ws:is_connected() do
    ws:pump(16)

    local line = io.read("*l")
    if line == nil then
      ws:close(1000, "stdin closed")
      break
    end

    if line == "/quit" then
      ws:close(1000, "client quit")
      break
    end

    if line ~= "" then
      ws.send.chat { text = line }
    end
  end

  return
end

local ui = rover.ui
local tui = rover.tui

local status = rover.signal("connecting")
local connected = rover.signal(false)
local draft = rover.signal("")
local messages = rover.signal({})
local selected = rover.signal(1)

local started = false
local pump_task = nil

local function append_message(line)
  local next = messages.val
  next[#next + 1] = line
  messages.val = next
  selected.val = math.max(1, #next)
end

local function send_chat(text)
  local line = tostring(text or "")
  if line == "" then
    return
  end

  if line == "/quit" then
    ws:close(1000, "client quit")
    return
  end

  if not ws:is_connected() then
    status.val = "not connected"
    return
  end

  ws.send.chat { text = line }
  append_message("[you] " .. line)
end

local function handle_key(key)
  if key == "enter" then
    local line = tostring(draft.val or "")
    send_chat(line)
    draft.val = ""
    return
  end

  if key == "backspace" then
    local line = tostring(draft.val or "")
    draft.val = line:sub(1, #line - 1)
    return
  end

  if key == "esc" then
    draft.val = ""
    return
  end

  local ch = tostring(key):match("^char:(.+)$")
  if ch ~= nil and #ch == 1 then
    draft.val = tostring(draft.val or "") .. ch
  end
end

function ws.join(ctx)
  connected.val = true
  status.val = "connected as " .. user_id
  ws.send.identify { user_id = user_id }
  append_message("[system] connected")

  if auto_text ~= nil and auto_text ~= "" then
    rover.spawn(function()
      rover.delay(200)
      send_chat(auto_text)
      if auto_quit then
        rover.delay(300)
        ws:close(1000, "auto quit")
      end
    end)
  end

  return { user_id = user_id }
end

function ws.listen.chat(msg, ctx, state)
  local who = tostring(msg.user_id or "anon")
  if who == user_id then
    return
  end
  append_message("[" .. who .. "] " .. tostring(msg.text or ""))
end

function ws.listen.user_joined(msg, ctx, state)
  local who = tostring(msg.user_id or "anon")
  if who ~= user_id then
    append_message("[join] " .. who)
  end
end

function ws.listen.user_left(msg, ctx, state)
  append_message("[left] " .. tostring(msg.user_id or "anon"))
end

function ws.error(err, ctx, state)
  status.val = "error"
  append_message("[error] " .. tostring(err.message or "unknown"))
end

function ws.leave(info, state)
  connected.val = false
  status.val = "closed"
  append_message("[system] closed " .. tostring(info.code or "") .. " " .. tostring(info.reason or ""))
end

local function start_client_once()
  if started then
    return
  end
  started = true

  ws:connect()

  pump_task = rover.interval(16, function()
    if ws:is_connected() then
      ws:pump(0)
    end
  end)

  rover.on_destroy(function()
    if pump_task ~= nil then
      rover.task.cancel(pump_task)
    end
    if ws:is_connected() then
      ws:close(1000, "shutdown")
    end
  end)
end

function rover.render()
  start_client_once()

  return tui.full_screen {
    tui.key_area {
      on_key = function(key)
        handle_key(key)
      end,
      ui.column {
        ui.text { "chat client" },
        tui.badge {
          label = rover.derive(function()
            if connected.val then
              return "online"
            end
            return "offline"
          end),
          tone = rover.derive(function()
            if connected.val then
              return "success"
            end
            return "warning"
          end),
        },
        ui.text { "user: " .. user_id },
        ui.text { status },
        tui.separator { width = 64, char = "-" },
        tui.nav_list {
          title = "messages",
          items = messages,
          selected = selected,
        },
        tui.separator { width = 64, char = "-" },
        ui.text { "type message, Enter send, /quit exits" },
        ui.text {
          rover.derive(function()
            return "> " .. tostring(draft.val or "")
          end),
        },
      },
    },
  }
end
