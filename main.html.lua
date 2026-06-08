local app = rover.server {}
local db = rover.db.connect()
local g = rover.guard

local function now()
	return os.date("!%Y-%m-%dT%H:%M:%SZ")
end

local function post_params(ctx)
	return ctx:body():expect {
		title = g:string():required "Title is required",
		body = g:string():required "Body is required",
	}
end

local function find_post(ctx)
	local id = tonumber(ctx:params().id)
	local post = db.posts:find():by_id(id):first()

	if not post then
		return nil, app:error(404, "Post not found")
	end

	return post
end

local function post_form(props)
	local post = props.post or {}
	local action = props.action
	local method = props.method or "post"
	local submit = props.submit or "Save Post"

	-- component() is the explored browser island boundary; signals stay client-only.
	return component(function()
		local title = rover.signal(post.title or "")
		local body = rover.signal(post.body or "")
		local blank = title == "" or body == ""

		return form {
			method = "post",
			action = action,

			when(method ~= "post", function()
				return input { type = "hidden", name = "_method", value = method }
			end),

			div {
				label { "Title", for_ = "post_title" },
				input {
					id = "post_title",
					name = "title",
					value = title,
					on_input = function(event)
						title.val = event.value
					end,
				},
			},

			div {
				label { "Body", for_ = "post_body" },
				textarea {
					id = "post_body",
					name = "body",
					body,
					on_input = function(event)
						body.val = event.value
					end,
				},
			},

			button {
				type = "submit",
				disabled = blank,
				submit,
			},

			when(blank, function()
				return p { "Title and body are required.", class = "error" }
			end),
		}
	end)
end

local function layout(title_text, children)
	return html {
		head {
			title(title_text),
		},
		body {
			main {
				class = "container",
				children,
			},
		},
	}
end

function app.get()
	return redirect "/posts"
end

function app.posts.get()
	local posts = db.posts:find():order_by(db.posts.created_at, "DESC"):all()

	return layout("Posts", {
		h1 "Posts",
		p "A Rails scaffold-shaped CRUD, written as plain Rover code.",
		a { "New post", href = "/posts/new", class = "button" },

		when(#posts == 0, function()
			return p "No posts yet. Create the first one."
		end),

		when(#posts > 0, function()
			return table {
				thead {
					tr {
						th "Title",
						th "Created",
						th "",
					},
				},
				tbody(each(posts, function(post)
					return tr {
						td(post.title),
						td(post.created_at),
						td {
							a { "Show", href = "/posts/" .. post.id },
							a { "Edit", href = "/posts/" .. post.id .. "/edit" },
							form {
								method = "post",
								action = "/api/posts/" .. post.id,
								input { type = "hidden", name = "_method", value = "delete" },
								button { type = "submit", "Destroy" },
							},
						},
					}
				end)),
			}
		end),
	})
end

function app.posts.new.get()
	return layout("New Post", {
		h1 "New Post",
		post_form {
			action = "/api/posts",
			submit = "Create Post",
		},
		a { "Back", href = "/posts" },
	})
end

function app.posts.p_id.get(ctx)
	local post, err = find_post(ctx)
	if err then
		return err
	end

	return layout(post.title, {
		h1(post.title),
		p(post.body),
		p { "Created at ", time { post.created_at, datetime = post.created_at } },
		a { "Edit", href = "/posts/" .. post.id .. "/edit" },
		a { "Back", href = "/posts" },
	})
end

function app.posts.p_id.edit.get(ctx)
	local post, err = find_post(ctx)
	if err then
		return err
	end

	return layout("Edit Post", {
		h1 "Edit Post",
		post_form {
			post = post,
			action = "/api/posts/" .. post.id,
			method = "patch",
			submit = "Update Post",
		},
		a { "Show", href = "/posts/" .. post.id },
		a { "Back", href = "/posts" },
	})
end

function app.api.posts.get()
	return app.json {
		posts = db.posts:find():order_by(db.posts.created_at, "DESC"):all(),
	}
end

function app.api.posts.post(ctx)
	local body = post_params(ctx)

	local post = db.posts:insert {
		title = body.title,
		body = body.body,
		created_at = now(),
		updated_at = now(),
	}

	return app.json:status(201, post)
end

function app.api.posts.p_id.get(ctx)
	local post, err = find_post(ctx)
	if err then
		return err
	end

	return app.json(post)
end

function app.api.posts.p_id.patch(ctx)
	local post, err = find_post(ctx)
	if err then
		return err
	end

	local body = post_params(ctx)

	db.posts:update()
		:by_id(post.id)
		:set {
			title = body.title,
			body = body.body,
			updated_at = now(),
		}
		:exec()

	return app.json(db.posts:find():by_id(post.id):first())
end

function app.api.posts.p_id.delete(ctx)
	local post, err = find_post(ctx)
	if err then
		return err
	end

	db.posts:delete():by_id(post.id):exec()

	return app.json { ok = true }
end

return app
