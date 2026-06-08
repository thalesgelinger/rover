local db = rover.db.connect()

local Posts = {}

local function now()
	return os.date("!%Y-%m-%dT%H:%M:%SZ")
end

function Posts.all()
	return db.posts:find():order_by(db.posts.created_at, "DESC"):all()
end

function Posts.find(id)
	return db.posts:find():by_id(id):first()
end

function Posts.create(attrs)
	local timestamp = now()

	return db.posts:insert {
		title = attrs.title,
		body = attrs.body,
		created_at = timestamp,
		updated_at = timestamp,
	}
end

function Posts.update(id, attrs)
	db.posts:update()
		:by_id(id)
		:set {
			title = attrs.title,
			body = attrs.body,
			updated_at = now(),
		}
		:exec()

	return Posts.find(id)
end

function Posts.delete(id)
	return db.posts:delete():by_id(id):exec()
end

return Posts
