local api = rover.server {}

local ShoppingCart = rover.component()

function ShoppingCart.init()
    return {
        products = {
            { id = 1, name = "Wireless Headphones", price = 79.99, category = "electronics", image = "🎧" },
            { id = 2, name = "Mechanical Keyboard", price = 129.99, category = "electronics", image = "⌨️" },
            { id = 3, name = "Coffee Mug", price = 14.99, category = "home", image = "☕" },
            { id = 4, name = "Desk Lamp", price = 34.99, category = "home", image = "💡" },
            { id = 5, name = "Notebook", price = 8.99, category = "office", image = "📓" },
            { id = 6, name = "Pen Set", price = 12.99, category = "office", image = "🖊️" },
        },
        cart = {},
        filter = "all"
    }
end

function ShoppingCart.addToCart(state, productId)
    for _, product in ipairs(state.products) do
        if product.id == productId then
            local newCart = {}
            for i, item in ipairs(state.cart) do
                newCart[i] = item
            end
            
            local found = false
            for _, item in ipairs(newCart) do
                if item.id == productId then
                    item.quantity = item.quantity + 1
                    found = true
                    break
                end
            end
            
            if not found then
                table.insert(newCart, { id = product.id, name = product.name, price = product.price, quantity = 1, image = product.image })
            end
            
            return { products = state.products, cart = newCart, filter = state.filter }
        end
    end
    return state
end

function ShoppingCart.removeFromCart(state, productId)
    local newCart = {}
    for _, item in ipairs(state.cart) do
        if item.id ~= productId then
            table.insert(newCart, item)
        end
    end
    return { products = state.products, cart = newCart, filter = state.filter }
end

function ShoppingCart.updateQuantity(state, productId, quantity)
    if quantity < 1 then
        return ShoppingCart.removeFromCart(state, productId)
    end
    
    local newCart = {}
    for _, item in ipairs(state.cart) do
        if item.id == productId then
            item.quantity = quantity
        end
        newCart[#newCart + 1] = item
    end
    return { products = state.products, cart = newCart, filter = state.filter }
end

function ShoppingCart.setFilter(state, filter)
    return { products = state.products, cart = state.cart, filter = filter }
end

function ShoppingCart.checkout(state)
    return { products = state.products, cart = {}, filter = state.filter }
end

function ShoppingCart.render(state)
    -- Filter products
    local filteredProducts = {}
    if state.filter == "all" then
        filteredProducts = state.products
    else
        for _, product in ipairs(state.products) do
            if product.category == state.filter then
                table.insert(filteredProducts, product)
            end
        end
    end
    
    -- Calculate totals
    local total = 0
    local count = 0
    for _, item in ipairs(state.cart) do
        total = total + (item.price * item.quantity)
        count = count + item.quantity
    end
    
    local data = {
        products = filteredProducts,
        cart = state.cart,
        filter = state.filter,
        total = total,
        count = count
    }
    
    return rover.html(data) [=[
        <div rover-data="{ filter: '{{ filter }}', cartOpen: false }"
             class="min-h-screen bg-gradient-to-br from-slate-900 to-slate-800 p-8">
            
            <div class="max-w-7xl mx-auto">
                <!-- Header -->
                <div class="flex justify-between items-center mb-8">
                    <h1 class="text-4xl font-bold text-white">🛒 Shopping Cart</h1>
                    <div class="relative">
                        <button rover-click="cartOpen = !cartOpen"
                                class="bg-white p-4 rounded-full shadow-lg hover:scale-110 transition-transform">
                            <span class="text-2xl">🛍️</span>
                            <span class="absolute -top-2 -right-2 bg-red-500 text-white text-sm font-bold rounded-full w-6 h-6 flex items-center justify-center" 
                                  x-text="{{ count }}"></span>
                        </button>
                        
                        <!-- Cart Dropdown -->
                        <div x-show="cartOpen" 
                             x-transition:enter="transition ease-out duration-200"
                             x-transition:enter-start="opacity-0 scale-95"
                             x-transition:enter-end="opacity-100 scale-100"
                             x-transition:leave="transition ease-in duration-150"
                             x-transition:leave-start="opacity-100 scale-100"
                             x-transition:leave-end="opacity-0 scale-95"
                             class="absolute right-0 mt-2 w-96 bg-white rounded-xl shadow-2xl p-6 z-50">
                            
                            {{ if count == 0 then }}
                                <p class="text-center text-gray-500 py-8">Your cart is empty</p>
                            {{ else }}
                                <h3 class="text-lg font-bold mb-4">Cart Items</h3>
                                <div class="max-h-64 overflow-y-auto space-y-3">
                                    {{ for _, item in ipairs(cart) do }}
                                        <div class="flex items-center gap-4 p-3 bg-gray-50 rounded-lg">
                                            <div class="text-3xl">{{ item.image }}</div>
                                            <div class="flex-1">
                                                <div class="font-medium">{{ item.name }}</div>
                                                <div class="text-sm text-gray-500">${{ string.format("%.2f", item.price) }} × {{ item.quantity }}</div>
                                            </div>
                                            <div class="text-right">
                                                <div class="font-bold">${{ string.format("%.2f", item.price * item.quantity) }}</div>
                                                <button rover-click="updateQuantity({{ item.id }}, {{ item.quantity - 1 }})"
                                                        class="text-red-500 hover:text-red-700 text-sm">−</button>
                                                <button rover-click="updateQuantity({{ item.id }}, {{ item.quantity + 1 }})"
                                                        class="text-green-500 hover:text-green-700 text-sm ml-2">+</button>
                                            </div>
                                        </div>
                                    {{ end }}
                                </div>
                                <div class="mt-4 pt-4 border-t">
                                    <div class="flex justify-between text-xl font-bold">
                                        <span>Total:</span>
                                        <span>${{ string.format("%.2f", total) }}</span>
                                    </div>
                                    <button rover-click="checkout"
                                            class="w-full mt-4 bg-green-500 hover:bg-green-600 text-white font-bold py-3 rounded-lg transition-colors">
                                        Checkout
                                    </button>
                                </div>
                            {{ end }}
                        </div>
                    </div>
                </div>
                
                <!-- Filter Tabs -->
                <div class="flex gap-2 mb-6">
                    <button rover-click="setFilter('all')"
                            class="px-6 py-2 rounded-full font-medium transition-colors {{ if filter == 'all' then }}bg-white text-slate-900{{ else }}bg-slate-700 text-slate-300 hover:bg-slate-600{{ end }}">
                        All
                    </button>
                    <button rover-click="setFilter('electronics')"
                            class="px-6 py-2 rounded-full font-medium transition-colors {{ if filter == 'electronics' then }}bg-white text-slate-900{{ else }}bg-slate-700 text-slate-300 hover:bg-slate-600{{ end }}">
                        Electronics
                    </button>
                    <button rover-click="setFilter('home')"
                            class="px-6 py-2 rounded-full font-medium transition-colors {{ if filter == 'home' then }}bg-white text-slate-900{{ else }}bg-slate-700 text-slate-300 hover:bg-slate-600{{ end }}">
                        Home
                    </button>
                    <button rover-click="setFilter('office')"
                            class="px-6 py-2 rounded-full font-medium transition-colors {{ if filter == 'office' then }}bg-white text-slate-900{{ else }}bg-slate-700 text-slate-300 hover:bg-slate-600{{ end }}">
                        Office
                    </button>
                </div>
                
                <!-- Product Grid -->
                <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
                    {{ for _, product in ipairs(products) do }}
                        <div class="bg-white rounded-2xl shadow-xl overflow-hidden hover:scale-105 transition-transform">
                            <div class="h-48 bg-gradient-to-br from-blue-100 to-purple-100 flex items-center justify-center">
                                <div class="text-8xl">{{ product.image }}</div>
                            </div>
                            <div class="p-6">
                                <span class="text-xs font-medium text-blue-600 uppercase tracking-wide">{{ product.category }}</span>
                                <h3 class="text-xl font-bold mt-2 mb-2">{{ product.name }}</h3>
                                <div class="flex justify-between items-center">
                                    <span class="text-2xl font-bold text-gray-900">${{ string.format("%.2f", product.price) }}</span>
                                    <button rover-click="addToCart({{ product.id }})"
                                            class="bg-blue-600 hover:bg-blue-700 text-white px-6 py-2 rounded-lg font-medium transition-colors">
                                        Add to Cart
                                    </button>
                                </div>
                            </div>
                        </div>
                    {{ end }}
                </div>
            </div>
        </div>
    ]=]
end

function api.get()
    local data = { ShoppingCart = ShoppingCart }
    return api.html(data) [=[
        <!DOCTYPE html>
        <html>
        <head>
            <title>Shopping Cart</title>
            <script src="https://cdn.tailwindcss.com"></script>
        </head>
        <body class="antialiased">
            {{ ShoppingCart() }}
        </body>
        </html>
    ]=]
end

return api
