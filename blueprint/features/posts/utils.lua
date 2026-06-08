local Posts = require "blueprint.features.posts.db"

local Utils = {}

function Utils.find_post(ctx)
	local post = Posts.find(tonumber(ctx:params().id))

	if not post then
		return nil, app:error(404, "Post not found")
	end

	return post
end

return Utils
