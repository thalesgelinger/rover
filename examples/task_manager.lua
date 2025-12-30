local api = rover.server {}

local TaskManager = rover.component()

function TaskManager.init()
    return {
        tasks = {},
        filter = "all",
        search = "",
        newTaskTitle = "",
        newTaskPriority = "medium"
    }
end

function TaskManager.addTask(state)
    if state.newTaskTitle == "" or state.newTaskTitle == nil then
        return state
    end
    
    local newTask = {
        id = #state.tasks + 1,
        title = state.newTaskTitle,
        priority = state.newTaskPriority,
        completed = false,
        createdAt = os.time()
    }
    
    local newTasks = {}
    for i, task in ipairs(state.tasks) do
        newTasks[i] = task
    end
    table.insert(newTasks, newTask)
    
    return {
        tasks = newTasks,
        filter = state.filter,
        search = state.search,
        newTaskTitle = "",
        newTaskPriority = state.newTaskPriority
    }
end

function TaskManager.toggleTask(state, taskId)
    local newTasks = {}
    for _, task in ipairs(state.tasks) do
        if task.id == taskId then
            task.completed = not task.completed
        end
        newTasks[#newTasks + 1] = task
    end
    return { tasks = newTasks, filter = state.filter, search = state.search, newTaskTitle = state.newTaskTitle, newTaskPriority = state.newTaskPriority }
end

function TaskManager.deleteTask(state, taskId)
    local newTasks = {}
    for _, task in ipairs(state.tasks) do
        if task.id ~= taskId then
            table.insert(newTasks, task)
        end
    end
    return { tasks = newTasks, filter = state.filter, search = state.search, newTaskTitle = state.newTaskTitle, newTaskPriority = state.newTaskPriority }
end

function TaskManager.setFilter(state, filter)
    return { tasks = state.tasks, filter = filter, search = state.search, newTaskTitle = state.newTaskTitle, newTaskPriority = state.newTaskPriority }
end

function TaskManager.setSearch(state, search)
    return { tasks = state.tasks, filter = state.filter, search = search, newTaskTitle = state.newTaskTitle, newTaskPriority = state.newTaskPriority }
end

function TaskManager.setPriority(state, priority)
    return { tasks = state.tasks, filter = state.filter, search = state.search, newTaskTitle = state.newTaskTitle, newTaskPriority = priority }
end

function TaskManager.render(state)
    -- Filter and search tasks
    local filteredTasks = {}
    for _, task in ipairs(state.tasks) do
        local matchesFilter = true
        if state.filter == "active" then
            matchesFilter = not task.completed
        elseif state.filter == "completed" then
            matchesFilter = task.completed
        end
        
        local matchesSearch = true
        if state.search ~= "" then
            local titleLower = string.lower(task.title)
            local searchLower = string.lower(state.search)
            matchesSearch = string.find(titleLower, searchLower) ~= nil
        end
        
        if matchesFilter and matchesSearch then
            table.insert(filteredTasks, task)
        end
    end
    
    -- Count stats
    local total = #state.tasks
    local completed = 0
    for _, task in ipairs(state.tasks) do
        if task.completed then
            completed = completed + 1
        end
    end
    
    local data = {
        tasks = filteredTasks,
        filter = state.filter,
        search = state.search,
        newTaskPriority = state.newTaskPriority,
        total = total,
        completed = completed,
        remaining = total - completed
    }
    
    return rover.html(data) [=[
        <div rover-data="{ newTaskTitle: '', search: '', newTaskPriority: 'medium', showForm: false, mobileMenuOpen: false }"
             class="min-h-screen bg-gradient-to-br from-indigo-900 via-purple-900 to-pink-900 p-4 md:p-8">
            
            <div class="max-w-4xl mx-auto">
                <!-- Header -->
                <div class="text-center mb-8">
                    <h1 class="text-4xl md:text-5xl font-bold text-white mb-2">📋 Task Manager</h1>
                    <p class="text-purple-200">Stay organized, get things done</p>
                </div>
                
                <!-- Stats Cards -->
                <div class="grid grid-cols-1 md:grid-cols-3 gap-4 mb-8">
                    <div class="bg-white/10 backdrop-blur-lg rounded-xl p-4 text-center border border-white/20">
                        <div class="text-3xl font-bold text-white">{{ total }}</div>
                        <div class="text-purple-200 text-sm">Total Tasks</div>
                    </div>
                    <div class="bg-white/10 backdrop-blur-lg rounded-xl p-4 text-center border border-white/20">
                        <div class="text-3xl font-bold text-green-400">{{ remaining }}</div>
                        <div class="text-purple-200 text-sm">Remaining</div>
                    </div>
                    <div class="bg-white/10 backdrop-blur-lg rounded-xl p-4 text-center border border-white/20">
                        <div class="text-3xl font-bold text-blue-400">{{ completed }}</div>
                        <div class="text-purple-200 text-sm">Completed</div>
                    </div>
                </div>
                
                <!-- Add Task Form -->
                <div class="bg-white rounded-2xl shadow-2xl p-6 mb-6">
                    <div class="flex flex-col md:flex-row gap-4">
                        <input type="text"
                               placeholder="What needs to be done?"
                               rover-model="newTaskTitle"
                               @keydown.enter.prevent="
                                   const title = newTaskTitle.trim();
                                   if (title) $rover.call('addTask');
                               "
                               class="flex-1 px-4 py-3 border-2 border-gray-200 rounded-xl focus:outline-none focus:border-purple-500 focus:ring-2 focus:ring-purple-200 transition-all" />
                        
                        <div class="flex gap-2">
                            <button rover-click="setPriority('low')"
                                    class="px-4 py-2 rounded-lg font-medium transition-colors {{ if newTaskPriority == 'low' then }}bg-blue-500 text-white{{ else }}bg-gray-100 text-gray-700 hover:bg-gray-200{{ end }}">
                                Low
                            </button>
                            <button rover-click="setPriority('medium')"
                                    class="px-4 py-2 rounded-lg font-medium transition-colors {{ if newTaskPriority == 'medium' then }}bg-yellow-500 text-white{{ else }}bg-gray-100 text-gray-700 hover:bg-gray-200{{ end }}">
                                Medium
                            </button>
                            <button rover-click="setPriority('high')"
                                    class="px-4 py-2 rounded-lg font-medium transition-colors {{ if newTaskPriority == 'high' then }}bg-red-500 text-white{{ else }}bg-gray-100 text-gray-700 hover:bg-gray-200{{ end }}">
                                High
                            </button>
                        </div>
                        
                        <button rover-click="addTask"
                                class="px-6 py-3 bg-purple-600 hover:bg-purple-700 text-white font-bold rounded-xl transition-colors">
                            Add
                        </button>
                    </div>
                </div>
                
                <!-- Search and Filter -->
                <div class="flex flex-col md:flex-row gap-4 mb-6">
                    <div class="flex-1 relative">
                        <span class="absolute left-3 top-1/2 -translate-y-1/2 text-gray-400">🔍</span>
                        <input type="text"
                               placeholder="Search tasks..."
                               rover-model="search"
                               class="w-full pl-10 pr-4 py-2 bg-white/10 border border-white/20 rounded-lg text-white placeholder-white/50 focus:outline-none focus:ring-2 focus:ring-purple-400" />
                    </div>
                    
                    <div class="flex gap-2 bg-white/10 p-1 rounded-lg">
                        <button rover-click="setFilter('all')"
                                class="px-4 py-2 rounded-md font-medium transition-colors {{ if filter == 'all' then }}bg-purple-600 text-white{{ else }}text-white/70 hover:text-white hover:bg-white/10{{ end }}">
                            All
                        </button>
                        <button rover-click="setFilter('active')"
                                class="px-4 py-2 rounded-md font-medium transition-colors {{ if filter == 'active' then }}bg-purple-600 text-white{{ else }}text-white/70 hover:text-white hover:bg-white/10{{ end }}">
                            Active
                        </button>
                        <button rover-click="setFilter('completed')"
                                class="px-4 py-2 rounded-md font-medium transition-colors {{ if filter == 'completed' then }}bg-purple-600 text-white{{ else }}text-white/70 hover:text-white hover:bg-white/10{{ end }}">
                            Completed
                        </button>
                    </div>
                </div>
                
                <!-- Task List -->
                <div class="space-y-3">
                    {{ if #tasks == 0 then }}
                        <div class="text-center py-12 bg-white/10 backdrop-blur-lg rounded-xl border border-white/20">
                            <div class="text-6xl mb-4">🎉</div>
                            <p class="text-white text-lg">No tasks found</p>
                            <p class="text-purple-200">{{ if filter == 'all' then }}Add a task to get started{{ else }}Change filter to see more tasks{{ end }}</p>
                        </div>
                    {{ else }}
                        {{ for _, task in ipairs(tasks) do }}
                            <div class="bg-white rounded-xl shadow-lg p-4 hover:shadow-xl transition-shadow {{ if task.completed then }}opacity-60{{ end }}">
                                <div class="flex items-center gap-4">
                                    <button rover-click="toggleTask({{ task.id }})"
                                            class="w-6 h-6 rounded-full border-2 flex items-center justify-center transition-colors {{ if task.completed then }}bg-green-500 border-green-500{{ else }}border-gray-300 hover:border-purple-500{{ end }}">
                                        {{ if task.completed then }}
                                            <svg class="w-4 h-4 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"></path>
                                            </svg>
                                        {{ end }}
                                    </button>
                                    
                                    <div class="flex-1 {{ if task.completed then }}line-through text-gray-500{{ end }}">
                                        <div class="font-medium text-lg">{{ task.title }}</div>
                                    </div>
                                    
                                    <span class="px-3 py-1 rounded-full text-xs font-medium {{ if task.priority == 'high' then }}bg-red-100 text-red-700{{ elseif task.priority == 'medium' then }}bg-yellow-100 text-yellow-700{{ else }}bg-blue-100 text-blue-700{{ end }}">
                                        {{ string.upper(task.priority) }}
                                    </span>
                                    
                                    <button rover-click="deleteTask({{ task.id }})"
                                            class="text-gray-400 hover:text-red-500 transition-colors p-2 hover:bg-red-50 rounded-lg">
                                        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"></path>
                                        </svg>
                                    </button>
                                </div>
                            </div>
                        {{ end }}
                    {{ end }}
                </div>
                
                <!-- Footer -->
                <div class="mt-8 text-center text-white/50 text-sm">
                    Built with Rover & Tailwind CSS
                </div>
            </div>
        </div>
    ]=]
end

function api.get()
    local data = { TaskManager = TaskManager }
    return api.html(data) [=[
        <!DOCTYPE html>
        <html>
        <head>
            <title>Task Manager</title>
            <script src="https://cdn.tailwindcss.com"></script>
        </head>
        <body class="antialiased">
            {{ TaskManager() }}
        </body>
        </html>
    ]=]
end

return api
