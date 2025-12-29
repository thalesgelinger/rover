
local api = rover.server {}

local TodoList = rover.component()

-- Initialize with empty todo list
function TodoList.init()
    return {
        todos = {},
        nextId = 1
    }
end

-- Add a new todo with the provided text
function TodoList.addTodo(state, todoText)
    -- Don't add empty todos
    if todoText == "" or todoText == nil then
        return state
    end

    -- Create new todo
    local newTodo = {
        id = state.nextId,
        text = todoText,
        completed = false
    }

    -- Add to todos list
    local newTodos = {}
    for i, todo in ipairs(state.todos) do
        newTodos[i] = todo
    end
    table.insert(newTodos, newTodo)

    return {
        todos = newTodos,
        nextId = state.nextId + 1
    }
end

-- Remove a todo by ID
function TodoList.removeTodo(state, todoId)
    local newTodos = {}
    for _, todo in ipairs(state.todos) do
        if todo.id ~= todoId then
            table.insert(newTodos, todo)
        end
    end

    return {
        todos = newTodos,
        nextId = state.nextId
    }
end

-- Toggle todo completion by ID
function TodoList.toggleTodo(state, todoId)
    local newTodos = {}
    for _, todo in ipairs(state.todos) do
        if todo.id == todoId then
            table.insert(newTodos, {
                id = todo.id,
                text = todo.text,
                completed = not todo.completed
            })
        else
            table.insert(newTodos, todo)
        end
    end

    return {
        todos = newTodos,
        nextId = state.nextId
    }
end

-- Clear all completed todos
function TodoList.clearCompleted(state)
    local newTodos = {}
    for _, todo in ipairs(state.todos) do
        if not todo.completed then
            table.insert(newTodos, todo)
        end
    end

    return {
        todos = newTodos,
        nextId = state.nextId
    }
end

function TodoList.render(state)
    -- Count active todos
    local activeCount = 0
    for _, todo in ipairs(state.todos) do
        if not todo.completed then
            activeCount = activeCount + 1
        end
    end

    local data = {
        todos = state.todos,
        hasTodos = #state.todos > 0,
        activeCount = activeCount
    }

    return rover.html(data) [=[
        <div style="max-width: 600px; margin: 0 auto; padding: 20px; font-family: Arial, sans-serif;">
            <h1 style="text-align: center; color: #333;">📝 Rover Todo List</h1>

            <!-- Input form -->
            <div style="display: flex; gap: 10px; margin-bottom: 20px;">
                <input
                    type="text"
                    id="todoInput"
                    placeholder="What needs to be done?"
                    style="flex: 1; padding: 12px; border: 2px solid #ddd; border-radius: 4px; font-size: 16px;"
                    onkeypress="if(event.key === 'Enter') handleAddTodo(event, this)"
                />
                <button
                    onclick="handleAddTodo(event, this)"
                    style="padding: 12px 24px; background: #4CAF50; color: white; border: none; border-radius: 4px; cursor: pointer; font-size: 16px; font-weight: bold;"
                >
                    Add
                </button>
            </div>

            <!-- Todo list -->
            {{ if hasTodos then }}
                <ul style="list-style: none; padding: 0;">
                    {{ for _, todo in ipairs(todos) do }}
                        <li style="display: flex; align-items: center; gap: 12px; padding: 12px; border: 1px solid #ddd; border-radius: 4px; margin-bottom: 8px; {{ if todo.completed then }}background: #f0f0f0;{{ end }}">
                            <input
                                type="checkbox"
                                onchange="handleToggle(event, this, {{ todo.id }})"
                                {{ if todo.completed then }}checked{{ end }}
                                style="width: 20px; height: 20px; cursor: pointer;"
                            />
                            <span style="flex: 1; {{ if todo.completed then }}text-decoration: line-through; color: #999;{{ end }}">
                                {{ todo.text }}
                            </span>
                            <button
                                onclick="removeTodo({{ todo.id }})"
                                style="padding: 6px 12px; background: #f44336; color: white; border: none; border-radius: 4px; cursor: pointer;"
                            >
                                Delete
                            </button>
                        </li>
                    {{ end }}
                </ul>

                <!-- Footer -->
                <div style="display: flex; justify-content: space-between; align-items: center; padding: 12px 0; border-top: 1px solid #ddd; margin-top: 20px;">
                    <span style="color: #666;">
                        {{ activeCount }} item{{ if activeCount ~= 1 then }}s{{ end }} left
                    </span>
                    <button
                        onclick="clearCompleted"
                        style="padding: 8px 16px; background: #ff9800; color: white; border: none; border-radius: 4px; cursor: pointer;"
                    >
                        Clear Completed
                    </button>
                </div>
            {{ else }}
                <p style="text-align: center; color: #999; padding: 40px 0;">
                    No todos yet. Add one above to get started!
                </p>
            {{ end }}
        </div>

        <script>
        function handleAddTodo(event, element) {
            // Get the input element
            const input = document.getElementById('todoInput');
            const todoText = input.value.trim();

            // Don't add empty todos
            if (!todoText) {
                return;
            }

            // Get component ID
            const container = element.closest('[data-rover-component]');
            const componentId = container.getAttribute('data-rover-component');

            // Call server with todo text
            roverEvent(event, componentId, 'addTodo', todoText);

            // Clear input immediately for better UX
            input.value = '';
        }

        function handleToggle(event, checkbox, todoId) {
            // Get component ID
            const container = checkbox.closest('[data-rover-component]');
            const componentId = container.getAttribute('data-rover-component');

            // Call server with todo ID
            roverEvent(event, componentId, 'toggleTodo', todoId);
        }
        </script>
    ]=]
end

function api.get()
    local data = { TodoList = TodoList }
    return api.html(data) [=[
        <!DOCTYPE html>
        <html>
        <head>
            <title>Rover Todo List</title>
        </head>
        <body style="background: #fafafa; margin: 0; padding: 0;">
            {{ TodoList() }}
        </body>
        </html>
    ]=]
end

return api
