(function () {
  function setupCopyInstallCommand() {
    var button = document.getElementById("copy-install-command");
    var command = document.getElementById("install-command");
    var copyIcon = document.getElementById("copy-icon");
    var checkIcon = document.getElementById("check-icon");

    if (!button || !command || !copyIcon || !checkIcon) {
      return;
    }

    button.addEventListener("click", function () {
      var text = command.textContent || "";
      if (!navigator.clipboard) {
        return;
      }
      navigator.clipboard.writeText(text).then(function () {
        copyIcon.classList.add("hidden");
        checkIcon.classList.remove("hidden");
        window.setTimeout(function () {
          checkIcon.classList.add("hidden");
          copyIcon.classList.remove("hidden");
        }, 2000);
      });
    });
  }

  function highlightLua(code) {
    var keywords = /\b(local|function|if|then|else|elseif|end|return|not|and|or|for|do|while|repeat|until|in|true|false|nil|require)\b/g;
    var strings = /("(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*')/g;
    var comments = /(--[^\n]*)/g;
    var funcCalls = /\b([a-zA-Z_]\w*)\s*(?=\()/g;

    var result = code.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
    result = result.replace(comments, '<span class="text-muted-foreground/50 italic">$1</span>');
    result = result.replace(strings, '<span class="text-primary">$1</span>');
    result = result.replace(keywords, '<span class="text-primary font-medium">$1</span>');
    result = result.replace(funcCalls, '<span class="text-foreground">$1</span>');
    return result;
  }

  function setupCodeTabs() {
    var tabs = [
      {
        name: "rest_api_basic.lua",
        code: "local api = rover.server {}\n\nfunction api.hello.get(ctx)\n  return api.json {\n    message = \"Hello World\"\n  }\nend\n\nfunction api.hello.p_id.get(ctx)\n  return api.json {\n    message = \"Hello \" .. ctx:params().id\n  }\nend\n\nfunction api.users.p_id.posts.p_postId.get(ctx)\n  local params = ctx:params()\n  return api.json {\n    message = \"User \" .. params.id .. \" - Post \" .. params.postId\n  }\nend\n\nreturn api",
      },
      {
        name: "validation_guard.lua",
        code: "local api = rover.server {}\nlocal g = rover.guard\n\nfunction api.users.post(ctx)\n  local ok, user = pcall(function()\n    return ctx:body():expect {\n      name = g:string():required(\"Missing name\"),\n      email = g:string():required(),\n      age = g:integer(),\n      tags = g:array(g:string())\n    }\n  end)\n\n  if not ok then\n    return api:error(400, user)\n  end\n\n  return api.json {\n    success = true,\n    user = user\n  }\nend\n\nreturn api",
      },
      {
        name: "db_example.lua",
        code: "local db = rover.db.connect()\n\nlocal user = db.users:insert {\n  name = \"Alice\",\n  age = 30,\n  email = \"alice@example.com\",\n  status = \"active\"\n}\n\nlocal active_users = db.users:find()\n  :by_status(\"active\")\n  :by_age_bigger_than(18)\n  :order_by(db.users.name, \"ASC\")\n  :limit(10)\n  :all()\n\nlocal order_summary = db.orders:find()\n  :group_by(db.orders.user_id)\n  :agg({\n    total = rover.db.sum(db.orders.amount),\n    count = rover.db.count(db.orders.id)\n  })\n  :having_total_bigger_than(100)\n  :all()",
      },
      {
        name: "tui/nav_list.lua",
        code: "local ui = rover.ui\nlocal tui = rover.tui\n\nfunction rover.render()\n  local items = rover.signal({\n    { id = \"parser\", label = \"Parser cleanup\" },\n    { id = \"tui\", label = \"TUI key routing\" },\n    { id = \"docs\", label = \"Docs update\" },\n  })\n  local selected = rover.signal(1)\n  local status = rover.signal(\"use arrows or j/k, enter to pick\")\n\n  return ui.column {\n    ui.text { \"nav_list example\" },\n    tui.nav_list {\n      title = \"Tasks\",\n      items = items,\n      selected = selected,\n      on_key = function(key)\n        if key == \"up\" or key == \"char:k\" then selected.val = math.max(1, selected.val - 1) end\n        if key == \"down\" or key == \"char:j\" then selected.val = math.min(#items.val, selected.val + 1) end\n        if key == \"enter\" then status.val = \"picked: \" .. tostring(items.val[selected.val].label) end\n      end,\n    },\n    ui.text { status },\n  }\nend",
      },
      {
        name: "websocket_server.lua",
        code: "local api = rover.server {}\n\nfunction api.chat.ws(ws)\n  function ws.join(ctx)\n    ws.send.connected {\n      message = \"WebSocket connection established\"\n    }\n    return {}\n  end\n\n  function ws.listen.message(msg, ctx, state)\n    ws.send.echo(msg)\n  end\n\n  function ws.leave(state)\n  end\nend\n\nreturn api",
      },
    ];

    var buttons = document.querySelectorAll(".code-tab-button");
    var codeContent = document.getElementById("code-content");
    if (!buttons.length || !codeContent) {
      return;
    }

    function renderTab(index) {
      buttons.forEach(function (button, i) {
        var active = i === index;
        var activeMarker = button.querySelector(".code-tab-active");
        button.classList.toggle("text-foreground", active);
        button.classList.toggle("text-muted-foreground", !active);
        button.classList.toggle("hover:text-foreground/70", !active);
        if (activeMarker) {
          activeMarker.classList.toggle("hidden", !active);
        }
      });
      codeContent.innerHTML = highlightLua(tabs[index].code);
    }

    buttons.forEach(function (button) {
      button.addEventListener("click", function () {
        var index = Number(button.getAttribute("data-tab-index")) || 0;
        renderTab(index);
      });
    });

    renderTab(0);
  }

  setupCopyInstallCommand();
  setupCodeTabs();
})();
