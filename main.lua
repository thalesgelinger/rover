local ru = rover.ui
local tui = rover.tui
local mod = rover.ui.mod
local fill = "█"

local function screen_size()
	local w = tonumber(ru.screen.width.val) or 0
	local h = tonumber(ru.screen.height.val) or 0
	return math.max(1, w), math.max(1, h)
end

local direction = rover.signal "right"
local snake = rover.signal {}
local food_x = rover.signal(0)
local food_y = rover.signal(0)
local score = rover.signal(0)
local length_text = rover.derive(function()
	return "len: " .. tostring(#snake.val)
end)
local score_text = rover.derive(function()
	return "score: " .. tostring(score.val)
end)
local started = false

local delta = {
	left = { -1, 0 },
	right = { 1, 0 },
	up = { 0, -1 },
	down = { 0, 1 },
}

local opposite = {
	left = "right",
	right = "left",
	up = "down",
	down = "up",
}

local function rand_pos()
	local w, h = screen_size()
	local min_y = h > 3 and 2 or 0
	return math.random(0, math.max(0, w - 1)), math.random(min_y, math.max(min_y, h - 1))
end

local function occupies(body, x, y)
	for i = 1, #body do
		local p = body[i]
		if p.x == x and p.y == y then
			return true
		end
	end
	return false
end

local function place_food()
	local body = snake.val
	for _ = 1, 200 do
		local x, y = rand_pos()
		if not occupies(body, x, y) then
			food_x.val = x
			food_y.val = y
			return
		end
	end
	food_x.val = 0
	food_y.val = 0
end

local function reset_game()
	local w, h = screen_size()
	local x, y = rand_pos()
	local body = {}
	for i = 0, 3 do
		body[#body + 1] = { x = (x - i) % w, y = y % h }
	end
	snake.val = body
	direction.val = "right"
	score.val = 0
	place_food()
end

local function tick()
	local body = snake.val
	if #body == 0 then
		reset_game()
		return
	end

	local d = delta[direction.val]
	if d == nil then
		return
	end

	local w, h = screen_size()
	local head = body[1]
	local nx = head.x + d[1]
	local ny = head.y + d[2]

	if nx < 0 then
		nx = math.max(0, w - 1)
	elseif nx >= w then
		nx = 0
	end
	if ny < 0 then
		ny = math.max(0, h - 1)
	elseif ny >= h then
		ny = 0
	end

	if occupies(body, nx, ny) then
		reset_game()
		return
	end

	local next = {
		{ x = nx, y = ny },
	}
	for i = 1, #body - 1 do
		next[#next + 1] = body[i]
	end

	if nx == food_x.val and ny == food_y.val then
		next[#next + 1] = body[#body]
		score.val = score.val + 1
		snake.val = next
		place_food()
		return
	end

	snake.val = next
end

local function on_key(key)
	if delta[key] == nil then
		return
	end
	if opposite[direction.val] == key then
		return
	end
	direction.val = key
end

local function start_once()
	if started then
		return
	end
	started = true
	math.randomseed(os.time())
	reset_game()
	rover.interval(140, tick)
end

function rover.render()
	start_once()

	return tui.full_screen {
		on_key = on_key,
		ru.stack {
			mod = mod:width("full"):height("full"):bg_color "surface",
			ru.view {
				mod = mod:position("absolute"):top(0):left(0),
				ru.text {
					score_text,
					mod = mod:color "text",
				},
			},
			ru.view {
				mod = mod:position("absolute"):top(1):left(0),
				ru.text {
					length_text,
					mod = mod:color "text",
				},
			},
			ru.view {
				mod = mod:position("absolute"):top(food_y):left(food_x),
				ru.text {
					fill,
					mod = mod:color "danger",
				},
			},
			ru.each(snake, function(segment, i)
				local color = i == 1 and "info" or "accent"
				return ru.view {
					mod = mod:position("absolute"):top(segment.y):left(segment.x),
					ru.text {
						fill,
						mod = mod:color(color),
					},
				}
			end, function(_, i)
				return i
			end),
		},
	}
end
