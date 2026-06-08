local Posts = require "blueprint.features.posts.db"
local Utils = require "blueprint.features.posts.utils"

local g = rover.guard

local post_params

function api.posts.get()
	return app.json {
		posts = Posts.all(),
	}
end

function api.posts.post(ctx)
	local post = Posts.create(post_params(ctx))

	return app.json:status(201, post)
end

function api.posts.p_id.get(ctx)
	local post, err = Utils.find_post(ctx)
	if err then
		return err
	end

	return app.json(post)
end

function api.posts.p_id.patch(ctx)
	local post, err = Utils.find_post(ctx)
	if err then
		return err
	end

	return app.json(Posts.update(post.id, post_params(ctx)))
end

function api.posts.p_id.delete(ctx)
	local post, err = Utils.find_post(ctx)
	if err then
		return err
	end

	Posts.delete(post.id)

	return app.json { ok = true }
end

function post_params(ctx)
	return ctx:body():expect {
		title = g:string():required "Title is required",
		body = g:string():required "Body is required",
	}
end
