local api = rover.server {}

local WeatherApp = rover.component()

function WeatherApp.init()
    return {
        city = "",
        loading = false,
        weather = nil,
        selectedUnit = "celsius",
        cities = {
            { name = "New York", temp = 12, condition = "Cloudy", humidity = 65 },
            { name = "London", temp = 8, condition = "Rainy", humidity = 80 },
            { name = "Tokyo", temp = 18, condition = "Sunny", humidity = 45 },
            { name = "Paris", temp = 14, condition = "Partly Cloudy", humidity = 55 },
            { name = "Sydney", temp = 22, condition = "Sunny", humidity = 40 }
        }
    }
end

function WeatherApp.setCity(state, city)
    return { city = city, selectedUnit = state.selectedUnit, weather = state.weather, cities = state.cities }
end

function WeatherApp.setUnit(state, unit)
    return { city = state.city, selectedUnit = unit, weather = state.weather, cities = state.cities }
end

function WeatherApp.fetchWeather(state)
    -- Simulate API call with delay
    local foundCity = nil
    for _, city in ipairs(state.cities) do
        if city.name == state.city then
            foundCity = city
            break
        end
    end
    
    return {
        city = state.city,
        loading = true,
        selectedUnit = state.selectedUnit,
        weather = foundCity,
        cities = state.cities
    }
end

function WeatherApp.convertTemp(tempC, unit)
    if unit == "celsius" then
        return tempC
    elseif unit == "fahrenheit" then
        return (tempC * 9/5) + 32
    else
        return tempC + 273.15
    end
end

function WeatherApp.getConditionIcon(condition)
    local icons = {
        ["Sunny"] = "☀️",
        ["Cloudy"] = "☁️",
        ["Partly Cloudy"] = "⛅",
        ["Rainy"] = "🌧️",
        ["Snowy"] = "❄️",
        ["Stormy"] = "⛈️"
    }
    return icons[condition] or "🌡️"
end

function WeatherApp.render(state)
    local weatherIcon = ""
    local tempDisplay = ""
    local feelsLike = ""
    
    if state.weather then
        weatherIcon = WeatherApp.getConditionIcon(state.weather.condition)
        tempDisplay = string.format("%.1f°", WeatherApp.convertTemp(state.weather.temp, state.selectedUnit))
        local feelsLikeC = state.weather.temp + (state.weather.humidity / 20)
        feelsLike = string.format("Feels like %.1f°", WeatherApp.convertTemp(feelsLikeC, state.selectedUnit))
    end
    
    local data = {
        city = state.city,
        loading = state.loading,
        weather = state.weather,
        selectedUnit = state.selectedUnit,
        cities = state.cities,
        weatherIcon = weatherIcon,
        tempDisplay = tempDisplay,
        feelsLike = feelsLike
    }
    
    return rover.html(data) [=[
        <div rover-data="{ showDetails: false }"
             class="min-h-screen bg-gradient-to-br from-blue-400 via-blue-500 to-purple-600">
            
            <div class="max-w-4xl mx-auto px-4 py-8">
                <!-- Header -->
                <div class="text-center mb-12">
                    <h1 class="text-4xl md:text-5xl font-bold text-white mb-2">🌤 Weather App</h1>
                    <p class="text-blue-100">Check weather in major cities</p>
                </div>
                
                <!-- Search Bar -->
                <div class="bg-white rounded-2xl shadow-2xl p-6 mb-8">
                    <div class="flex gap-3">
                        <input type="text"
                               placeholder="Enter city name..."
                               rover-model="city"
                               @keydown.enter.prevent="
                                   const city = city.trim();
                                   if (city) $rover.call('fetchWeather');
                               "
                               class="flex-1 px-4 py-3 border-2 border-gray-200 rounded-xl focus:outline-none focus:border-blue-500 focus:ring-2 focus:ring-blue-200 text-lg" />
                        
                        <button rover-click="fetchWeather"
                                class="px-8 py-3 bg-blue-600 hover:bg-blue-700 text-white font-bold rounded-xl transition-colors">
                            Search
                        </button>
                    </div>
                    
                    <!-- Quick Select -->
                    <div class="flex flex-wrap gap-2 mt-4">
                        {{ for _, city in ipairs(cities) do }}
                            <button rover-click="setCity('{{ city.name }}')"
                                    class="px-4 py-2 rounded-full text-sm font-medium transition-colors {{ if city == city.name then }}bg-blue-600 text-white{{ else }}bg-gray-100 text-gray-700 hover:bg-gray-200{{ end }}">
                                {{ city.name }}
                            </button>
                        {{ end }}
                    </div>
                </div>
                
                <!-- Loading State -->
                <div x-show="loading"
                     class="bg-white rounded-2xl shadow-2xl p-12 mb-8 text-center">
                    <div class="animate-spin rounded-full h-12 w-12 border-b-2 border-blue-600 mx-auto mb-4"></div>
                    <p class="text-gray-600 text-lg">Fetching weather data...</p>
                </div>
                
                <!-- Weather Display -->
                <div x-show="weather && !loading" 
                     x-transition:enter="transition ease-out duration-300"
                     x-transition:enter-start="opacity-0 translate-y-4"
                     x-transition:enter-end="opacity-100 translate-y-0"
                     class="bg-white rounded-2xl shadow-2xl overflow-hidden">
                    
                    <!-- Main Weather Info -->
                    <div class="bg-gradient-to-r from-blue-500 to-purple-600 p-8 text-center">
                        <div class="text-8xl mb-4">{{ weatherIcon }}</div>
                        <h2 class="text-4xl md:text-5xl font-bold text-white mb-2">{{ city }}</h2>
                        <div class="text-7xl md:text-8xl font-light text-white mb-2">{{ tempDisplay }}</div>
                        <p class="text-blue-100 text-xl">{{ weather.condition }}</p>
                    </div>
                    
                    <!-- Weather Details -->
                    <div class="p-6">
                        <!-- Unit Toggle -->
                        <div class="flex justify-center gap-2 mb-6">
                            <button rover-click="setUnit('celsius')"
                                    class="px-6 py-2 rounded-lg font-medium transition-colors {{ if selectedUnit == 'celsius' then }}bg-blue-600 text-white{{ else }}bg-gray-100 text-gray-700 hover:bg-gray-200{{ end }}">
                                °C
                            </button>
                            <button rover-click="setUnit('fahrenheit')"
                                    class="px-6 py-2 rounded-lg font-medium transition-colors {{ if selectedUnit == 'fahrenheit' then }}bg-blue-600 text-white{{ else }}bg-gray-100 text-gray-700 hover:bg-gray-200{{ end }}">
                                °F
                            </button>
                            <button rover-click="setUnit('kelvin')"
                                    class="px-6 py-2 rounded-lg font-medium transition-colors {{ if selectedUnit == 'kelvin' then }}bg-blue-600 text-white{{ else }}bg-gray-100 text-gray-700 hover:bg-gray-200{{ end }}">
                                K
                            </button>
                        </div>
                        
                        <!-- Stats Grid -->
                        <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
                            <div class="bg-blue-50 rounded-xl p-4 text-center">
                                <div class="text-4xl mb-2">💧</div>
                                <div class="text-sm text-blue-600 mb-1">Humidity</div>
                                <div class="text-2xl font-bold text-gray-800">{{ weather.humidity }}%</div>
                            </div>
                            
                            <div class="bg-green-50 rounded-xl p-4 text-center">
                                <div class="text-4xl mb-2">🌡️</div>
                                <div class="text-sm text-green-600 mb-1">Feels Like</div>
                                <div class="text-2xl font-bold text-gray-800">{{ feelsLike }}</div>
                            </div>
                            
                            <div class="bg-purple-50 rounded-xl p-4 text-center">
                                <div class="text-4xl mb-2">🌬️</div>
                                <div class="text-sm text-purple-600 mb-1">Wind</div>
                                <div class="text-2xl font-bold text-gray-800">12 km/h</div>
                            </div>
                        </div>
                        
                        <!-- Forecast -->
                        <div class="mt-6">
                            <h3 class="text-lg font-bold text-gray-800 mb-4">5-Day Forecast</h3>
                            <div class="grid grid-cols-5 gap-3">
                                <div class="text-center bg-gray-50 rounded-lg p-3">
                                    <div class="text-sm text-gray-600">Mon</div>
                                    <div class="text-3xl my-2">☀️</div>
                                    <div class="font-bold">{{ tempDisplay }}</div>
                                </div>
                                <div class="text-center bg-gray-50 rounded-lg p-3">
                                    <div class="text-sm text-gray-600">Tue</div>
                                    <div class="text-3xl my-2">⛅</div>
                                    <div class="font-bold">{{ tempDisplay }}</div>
                                </div>
                                <div class="text-center bg-gray-50 rounded-lg p-3">
                                    <div class="text-sm text-gray-600">Wed</div>
                                    <div class="text-3xl my-2">☁️</div>
                                    <div class="font-bold">{{ tempDisplay }}</div>
                                </div>
                                <div class="text-center bg-gray-50 rounded-lg p-3">
                                    <div class="text-sm text-gray-600">Thu</div>
                                    <div class="text-3xl my-2">🌧️</div>
                                    <div class="font-bold">{{ tempDisplay }}</div>
                                </div>
                                <div class="text-center bg-gray-50 rounded-lg p-3">
                                    <div class="text-sm text-gray-600">Fri</div>
                                    <div class="text-3xl my-2">☀️</div>
                                    <div class="font-bold">{{ tempDisplay }}</div>
                                </div>
                            </div>
                        </div>
                    </div>
                </div>
                
                <!-- Empty State -->
                <div x-show="!weather && !loading"
                     class="bg-white rounded-2xl shadow-2xl p-12 text-center">
                    <div class="text-6xl mb-4">🌡️</div>
                    <h2 class="text-2xl font-bold text-gray-800 mb-2">No City Selected</h2>
                    <p class="text-gray-600">Search for a city or select one from the quick links above</p>
                </div>
                
                <!-- Footer -->
                <div class="mt-8 text-center text-white/70 text-sm">
                    Built with Rover, Alpine.js & Tailwind CSS
                </div>
            </div>
        </div>
    ]=]
end

function api.get()
    local data = { WeatherApp = WeatherApp }
    return api.html(data) [=[
        <!DOCTYPE html>
        <html>
        <head>
            <title>Weather App</title>
            <script src="https://cdn.tailwindcss.com"></script>
        </head>
        <body class="antialiased">
            {{ WeatherApp() }}
        </body>
        </html>
    ]=]
end

return api
