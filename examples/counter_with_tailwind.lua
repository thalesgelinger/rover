local api = rover.server {}

local Counter = rover.component()

function Counter.init()
    return { count = 0, step = 1 }
end

function Counter.increment(state)
    return { count = state.count + state.step, step = state.step }
end

function Counter.decrement(state)
    return { count = state.count - state.step, step = state.step }
end

function Counter.reset(state)
    return { count = 0, step = state.step }
end

function Counter.setStep(state, newStep)
    return { count = state.count, step = newStep }
end

function Counter.render(state)
    local data = {
        count = state.count,
        step = state.step,
        stepText = "Step: " .. state.step
    }
    
    return rover.html(data) [=[
        <div rover-data="count: 0, step: 1"
             class="min-h-screen flex items-center justify-center bg-gradient-to-br from-purple-500 to-pink-500">
            
            <div class="bg-white rounded-2xl shadow-2xl p-8 max-w-md w-full">
                <h1 class="text-3xl font-bold text-center mb-8 text-gray-800">Tailwind Counter</h1>
                
                <!-- Counter Display -->
                <div class="text-6xl font-bold text-center mb-8">{{ count }}</div>
                
                <!-- Step Controls -->
                <div class="mb-6">
                    <label class="block text-sm font-medium text-gray-700 mb-2">Step Size</label>
                    <input type="range" min="1" max="10" value="1"
                           rover-model="step"
                           class="w-full h-2 bg-gray-200 rounded-lg appearance-none cursor-pointer accent-purple-600">
                    <div class="text-center mt-2 text-gray-600" x-text="`Step: ${step}`"></div>
                </div>
                
                <!-- Action Buttons -->
                <div class="flex gap-4">
                    <button rover-click="decrement"
                            class="flex-1 bg-red-500 hover:bg-red-600 text-white font-bold py-3 px-6 rounded-lg transition-colors">
                        −
                    </button>
                    <button rover-click="reset"
                            class="flex-1 bg-gray-500 hover:bg-gray-600 text-white font-bold py-3 px-6 rounded-lg transition-colors">
                        Reset
                    </button>
                    <button rover-click="increment"
                            class="flex-1 bg-blue-500 hover:bg-blue-600 text-white font-bold py-3 px-6 rounded-lg transition-colors">
                        +
                    </button>
                </div>
                
                <!-- Color Preview -->
                <div class="mt-6 p-4 rounded-lg {{ if count > 0 then }}bg-green-100 text-green-800{{ else }}bg-gray-100 text-gray-600{{ end }}">
                    <p class="text-center font-medium">
                        {{ if count == 0 then }}Ready to count!{{ else }}Count is positive: {{ count }}{{ end }}
                    </p>
                </div>
            </div>
        </div>
    ]=]
end

function api.get()
    local data = { Counter = Counter }
    return api.html(data) [=[
        <!DOCTYPE html>
        <html>
        <head>
            <title>Tailwind Counter</title>
            <script src="https://cdn.tailwindcss.com"></script>
        </head>
        <body class="antialiased">
            {{ Counter() }}
        </body>
        </html>
    ]=]
end

return api
