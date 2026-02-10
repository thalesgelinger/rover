require "rover.tui"
local ru = rover.ui
local mod = rover.ui.mod

local function screen_size()
	return ru.screen.width.val, ru.screen.height.val
end

local function clamp(n, minv, maxv)
	return math.max(minv, math.min(n, maxv))
end

local direction = rover.signal "up"

local delta = {
	left = { -1, 0 },
	right = { 1, 0 },
	up = { 0, -1 },
	down = { 0, 1 },
}

function rover.render()
	local w, h = screen_size()
	local x = rover.signal(math.floor(w / 2))
	local y = rover.signal(math.floor(h / 2))

	rover.interval(100, function()
		local d = delta[direction.val]
		if d == nil then
			return
		end

		local sw, sh = screen_size()
		x.val = clamp(x.val + d[1], 0, math.max(0, sw - 1))
		y.val = clamp(y.val + d[2], 0, math.max(0, sh - 1))
	end)

	local function on_key(key)
		if delta[key] ~= nil then
			direction.val = key
		end
	end

	return ru.full_screen {
		on_key = on_key,
		ru.stack {
			mod = mod:width("full"):height("full"):bg_color "surface",
			ru.view {
				mod = mod:position("absolute"):top(y):left(x),
				ru.text {
					"*",
					mod = mod:color "#00ffff",
				},
			},
		},
	}
end
