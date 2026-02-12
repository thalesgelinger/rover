require "rover.tui"
local ru = rover.ui
local mod = rover.ui.mod
local fill = "█"
local tick_ms = math.floor(1000 / 18)

math.randomseed(os.time())

function rover.render()
	local direction = rover.signal("right")
	local queued_direction = rover.signal("right")
	local game_over = rover.signal(false)
	local score = rover.signal(0)

	local board_w, board_h = screen_size()
	local play_top = 3
	local play_h = math.max(4, board_h - play_top + 1)

	local initial_segments = {
		{ x = math.max(2, math.floor(board_w / 2)), y = math.max(play_top, math.floor(play_top + play_h / 2)) },
		{ x = math.max(1, math.floor(board_w / 2) - 1), y = math.max(play_top, math.floor(play_top + play_h / 2)) },
	}

	local segments = rover.signal(initial_segments)
	local food = random_food(initial_segments, board_w, play_top, play_h)
	local food_x = rover.signal(food.x)
	local food_y = rover.signal(food.y)

	local function reset_game()
		direction.val = "right"
		queued_direction.val = "right"
		segments.val = {
			{ x = math.max(2, math.floor(board_w / 2)), y = math.max(play_top, math.floor(play_top + play_h / 2)) },
			{ x = math.max(1, math.floor(board_w / 2) - 1), y = math.max(play_top, math.floor(play_top + play_h / 2)) },
		}
		score.val = 0
		game_over.val = false
		local new_food = random_food(segments.val, board_w, play_top, play_h)
		food_x.val = new_food.x
		food_y.val = new_food.y
	end

	local function is_reverse(curr, next)
		if curr == "left" and next == "right" then
			return true
		end
		if curr == "right" and next == "left" then
			return true
		end
		if curr == "up" and next == "down" then
			return true
		end
		if curr == "down" and next == "up" then
			return true
		end
		return false
	end

	function on_key(key)
		local directions = {
			up = true,
			down = true,
			left = true,
			right = true,
		}

		if key == "char:r" then
			reset_game()
			return
		end

		if game_over.val and key == "enter" then
			reset_game()
			return
		end

		if directions[key] then
			if not game_over.val and not is_reverse(direction.val, key) then
				queued_direction.val = key
			end
		end
	end

	rover.interval(tick_ms, function()
		if game_over.val then
			return
		end

		local body = segments.val
		local head = body[1]
		local dir = queued_direction.val
		direction.val = dir

		local new_head = { x = head.x, y = head.y }
		if dir == "right" then
			new_head.x = head.x + 1
		elseif dir == "left" then
			new_head.x = head.x - 1
		elseif dir == "up" then
			new_head.y = head.y - 1
		elseif dir == "down" then
			new_head.y = head.y + 1
		end

		if new_head.x < 1 then
			new_head.x = board_w
		elseif new_head.x > board_w then
			new_head.x = 1
		end

		if new_head.y < play_top then
			new_head.y = board_h
		elseif new_head.y > board_h then
			new_head.y = play_top
		end

		for i = 1, #body do
			local part = body[i]
			if part.x == new_head.x and part.y == new_head.y then
				game_over.val = true
				return
			end
		end

		local ate = new_head.x == food_x.val and new_head.y == food_y.val
		local new_body = { new_head }
		local keep_len = #body - 1
		if ate then
			keep_len = #body
		end

		for i = 1, keep_len do
			new_body[i + 1] = body[i]
		end

		segments.val = new_body

		if ate then
			score.val = score.val + 1
			if #new_body >= board_w * play_h then
				game_over.val = true
				return
			end
			local next_food = random_food(new_body, board_w, play_top, play_h)
			food_x.val = next_food.x
			food_y.val = next_food.y
		end
	end)

	return ru.full_screen {
		on_key = on_key,
		ru.stack {
			mod = mod:width("full"):height("full"):bg_color "surface",
			ru.text {
				rover.derive(function()
					if game_over.val then
						return "Score: " .. score.val .. "  |  GAME OVER - Enter/R to restart"
					end
					return "Score: " .. score.val .. "  |  Arrows move  |  R restart"
				end),
				mod = mod:color "#9dd3ff",
			},
			ru.text {
				"",
			},
			FoodPiece { x = food_x, y = food_y },
			ru.each(segments, function(item)
				return SnakePiece(item)
			end),
		},
	}
end

function SnakePiece(props)
	return ru.view {
		mod = mod:position("absolute"):left(props.x):top(props.y),
		ru.text {
			fill,
			mod = mod:color "#00ff00",
		},
	}
end

function FoodPiece(props)
	return ru.view {
		mod = mod:position("absolute"):left(props.x):top(props.y):bg_color"surface",
		ru.text {
			"●",
			mod = mod:bg_color("surface"):color "#ff4d4d",
		},
	}
end

function random_food(body, w, play_top, play_h)
	local y_max = play_top + play_h - 1
	local occupied = {}
	for i = 1, #body do
		local segment = body[i]
		occupied[segment.x .. ":" .. segment.y] = true
	end

	local max_tries = 256
	for _ = 1, max_tries do
		local x = math.random(1, w)
		local y = math.random(play_top, y_max)
		local key = x .. ":" .. y
		if not occupied[key] then
			return { x = x, y = y }
		end
	end

	for y = play_top, y_max do
		for x = 1, w do
			local key = x .. ":" .. y
			if not occupied[key] then
				return { x = x, y = y }
			end
		end
	end

	return { x = 1, y = play_top }
end

function screen_size()
	return ru.screen.width.val, ru.screen.height.val
end
