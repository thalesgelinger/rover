local Posts = require "blueprint.features.posts.db"
local Utils = require "blueprint.features.posts.utils"

local layout
local post_form

function app.get()
	return redirect "/posts"
end

function app.posts.get()
	local posts = Posts.all()

	return layout("Posts", {
		h1 "Posts",
		p "A Rails scaffold-shaped CRUD, split by feature.",
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
	local post, err = Utils.find_post(ctx)
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
	local post, err = Utils.find_post(ctx)
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

function layout(title_text, children)
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

function post_form(props)
	local post = props.post or {}
	local action = props.action
	local method = props.method or "post"
	local submit = props.submit or "Save Post"

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
