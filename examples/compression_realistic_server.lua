-- Realistic compression example: E-commerce API
-- Demonstrates compression config in a production-like server with DB, auth, and multiple endpoints
--
-- Run:
--   cargo run -p rover_cli -- run examples/compression_realistic_server.lua
--
-- Test with curl:
--   # Large catalog - will be compressed
--   curl -H "Accept-Encoding: gzip" http://localhost:8080/api/products --compressed -v
--
--   # Small response - won't be compressed (< min_size)
--   curl -H "Accept-Encoding: gzip" http://localhost:8080/api/health --compressed -v
--
--   # With auth token
--   curl -H "Authorization: Bearer demo-token" -H "Accept-Encoding: deflate" \
--        http://localhost:8080/api/orders --compressed -v

local api = rover.server {
	host = "0.0.0.0",
	port = 8080,
	log_level = "info",
	compress = {
		enabled = true,
		algorithms = { "gzip", "deflate" },
		min_size = 512,
		types = {
			"application/json",
			"text/plain",
			"text/html",
		},
	},
}

local db = rover.db.connect()

-- ============================================================================
-- Authentication
-- ============================================================================

local function require_auth(ctx)
	local auth_header = ctx:headers()["Authorization"]
	if not auth_header then
		return nil, api.json:status(401, { error = "Missing authorization header" })
	end

	local token = auth_header:match "Bearer%s+(.+)$"
	if not token then
		return nil, api.json:status(401, { error = "Invalid authorization format. Use 'Bearer <token>'" })
	end

	if token ~= "demo-token" and token ~= "admin-token" then
		return nil, api.json:status(401, { error = "Invalid token" })
	end

	return { token = token, role = token == "admin-token" and "admin" or "user" }
end

-- ============================================================================
-- Public Endpoints
-- ============================================================================

-- Health check - small response, won't be compressed
function api.api.health.get(ctx)
	return api.json {
		status = "healthy",
		timestamp = os.date "!%Y-%m-%dT%H:%M:%SZ",
		version = "1.0.0",
	}
end

-- Product catalog - large response, will be compressed
function api.api.products.get(ctx)
	local query = ctx:query()

	local products_query = db.products:find()

	if query.category then
		products_query = products_query:by_category(query.category)
	end

	if query.search then
		products_query = products_query:by_name_contains(query.search)
	end

	local all_products = products_query:all()

	return api.json {
		products = all_products,
		count = #all_products,
	}
end

-- Single product - may or may not be compressed based on size
function api.api.products.p_id.get(ctx)
	local id = tonumber(ctx:params().id)
	local product = db.products:find():by_id(id):first()

	if not product then
		return api.json:status(404, { error = "Product not found" })
	end

	return api.json(product)
end

-- ============================================================================
-- Authenticated Endpoints
-- ============================================================================

-- User orders - compressed (likely large)
function api.api.orders.get(ctx)
	local user, err = require_auth(ctx)
	if not user then
		return err
	end

	local orders = db.orders:find():all()

	local enriched_orders = {}
	for _, order in ipairs(orders) do
		local items = db.order_items:find():by_order_id(order.id):all()
		table.insert(enriched_orders, {
			id = order.id,
			status = order.status,
			total = order.total,
			created_at = order.created_at,
			items = items,
		})
	end

	return api.json {
		orders = enriched_orders,
		count = #enriched_orders,
	}
end

-- Create order - request body validation
function api.api.orders.post(ctx)
	local user, err = require_auth(ctx)
	if not user then
		return err
	end

	local body = ctx:body():json()

	if not body.items or #body.items == 0 then
		return api.json:status(400, { error = "Items array is required" })
	end

	if not body.shipping_address or body.shipping_address == "" then
		return api.json:status(400, { error = "Shipping address is required" })
	end

	local total = 0
	local order_items = {}

	for _, item in ipairs(body.items) do
		if not item.product_id or not item.quantity then
			return api.json:status(400, { error = "Each item must have product_id and quantity" })
		end

		if item.quantity < 1 then
			return api.json:status(400, { error = "Quantity must be at least 1" })
		end

		local product = db.products:find():by_id(item.product_id):first()
		if not product then
			return api.json:status(400, {
				error = "Invalid product_id",
				product_id = item.product_id,
			})
		end

		local item_total = product.price * item.quantity
		total = total + item_total

		table.insert(order_items, {
			product_id = item.product_id,
			product_name = product.name,
			quantity = item.quantity,
			unit_price = product.price,
			total = item_total,
		})
	end

	local order = db.orders:insert {
		user_token = user.token,
		status = "pending",
		total = total,
		shipping_address = body.shipping_address,
		created_at = os.date "!%Y-%m-%dT%H:%M:%SZ",
	}

	for _, item in ipairs(order_items) do
		db.order_items:insert {
			order_id = order.id,
			product_id = item.product_id,
			quantity = item.quantity,
			unit_price = item.unit_price,
			total = item.total,
		}
	end

	return api.json:status(201, {
		order = order,
		items = order_items,
		message = "Order created successfully",
	})
end

-- ============================================================================
-- Admin Endpoints
-- ============================================================================

-- Analytics dashboard - very large response, definitely compressed
function api.api.admin.analytics.get(ctx)
	local user, err = require_auth(ctx)
	if not user then
		return err
	end

	if user.role ~= "admin" then
		return api.json:status(403, { error = "Admin access required" })
	end

	local products = db.products:find():all()
	local orders = db.orders:find():all()

	local category_stats = {}
	for _, product in ipairs(products) do
		category_stats[product.category] = (category_stats[product.category] or 0) + 1
	end

	local daily_revenue = {}
	for _, order in ipairs(orders) do
		local date = string.sub(order.created_at, 1, 10)
		daily_revenue[date] = (daily_revenue[date] or 0) + order.total
	end

	local top_products = {}
	local product_sales = {}
	local items = db.order_items:find():all()
	for _, item in ipairs(items) do
		product_sales[item.product_id] = (product_sales[item.product_id] or 0) + item.quantity
	end

	for product_id, quantity in pairs(product_sales) do
		local product = db.products:find():by_id(product_id):first()
		if product then
			table.insert(top_products, {
				product_id = product_id,
				name = product.name,
				total_quantity = quantity,
				revenue = quantity * product.price,
			})
		end
	end

	return api.json {
		summary = {
			total_products = #products,
			total_orders = #orders,
		},
		category_distribution = category_stats,
		daily_revenue = daily_revenue,
		top_products = top_products,
		generated_at = os.date "!%Y-%m-%dT%H:%M:%SZ",
	}
end

-- ============================================================================
-- Non-Compressed Endpoints (demonstrate when compression is skipped)
-- ============================================================================

-- Plain text endpoint - demonstrates text/plain compression
function api.api.docs.get(ctx)
	local docs = [[
API Documentation
==================

Public Endpoints:
  GET /api/health           - Health check (small, not compressed)
  GET /api/products         - Product catalog (large, compressed)
  GET /api/products/:id     - Single product

Authenticated Endpoints:
  GET /api/orders           - List orders (compressed)
  POST /api/orders          - Create order

Admin Endpoints:
  GET /api/admin/analytics  - Analytics dashboard (compressed)

Compression:
- gzip and deflate supported
- Minimum size: 512 bytes
- SSE streaming is never compressed
]]

	return api.text(docs)
end

-- ============================================================================
-- Seed Data (runs on startup)
-- ============================================================================

print "Seeding database with sample data..."

local categories = { "electronics", "clothing", "books", "home", "sports" }
local adjectives = { "Premium", "Basic", "Advanced", "Compact", "Professional", "Smart", "Eco", "Luxury" }
local nouns = { "Widget", "Gadget", "Device", "Tool", "Kit", "System", "Solution", "Product" }

for i = 1, 100 do
	local category = categories[(i % #categories) + 1]
	local adj = adjectives[(i % #adjectives) + 1]
	local noun = nouns[(i % #nouns) + 1]

	db.products:insert {
		name = adj .. " " .. noun .. " " .. i,
		category = category,
		price = ((i * 137) % 49000 + 1000) / 100,
		description = "A high-quality "
			.. category
			.. " "
			.. noun:lower()
			.. " designed for everyday use. Features include durability, "
			.. "performance optimization, and user-friendly design. "
			.. "Perfect for both beginners and professionals.",
		stock = (i * 43) % 100,
		sku = string.upper(string.sub(category, 1, 3)) .. "-" .. string.format("%06d", i),
		created_at = os.date "!%Y-%m-%dT%H:%M:%SZ",
	}
end

print "Created 100 products"

for i = 1, 50 do
	db.orders:insert {
		user_token = i % 2 == 0 and "demo-token" or "admin-token",
		status = ({ "pending", "processing", "shipped", "delivered", "cancelled" })[(i % 5) + 1],
		total = ((i * 213) % 45000 + 5000) / 100,
		shipping_address = "123 Main St, City " .. tostring(i) .. ", Country",
		created_at = os.date "!%Y-%m-%dT%H:%M:%SZ",
	}
end

print "Created 50 orders"

for i = 1, 150 do
	local order_id = (i % 50) + 1
	local product_id = (i % 100) + 1
	local quantity = (i % 5) + 1
	local price = ((i * 89) % 9000 + 1000) / 100

	db.order_items:insert {
		order_id = order_id,
		product_id = product_id,
		quantity = quantity,
		unit_price = price,
		total = price * quantity,
	}
end

print "Created 150 order items"
print "Server ready!"
print ""
print "Test commands:"
print "  curl -H 'Accept-Encoding: gzip' http://localhost:8080/api/products | wc -c"
print "  curl -H 'Authorization: Bearer demo-token' -H 'Accept-Encoding: deflate' http://localhost:8080/api/orders | wc -c"
print "  curl -H 'Accept-Encoding: gzip' http://localhost:8080/api/health -v  # Not compressed (small)"

return api
