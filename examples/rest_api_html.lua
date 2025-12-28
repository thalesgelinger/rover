local api = rover.server {}

-- Define a reusable card component
function rover.html.card(props)
    return rover.html(props) [=[
        <div class="card">
            <h2>{{ title }}</h2>
            <div class="card-content">{{ content }}</div>
        </div>
    ]=]
end

-- Define a list item component
function rover.html.list_item(props)
    return rover.html(props) [=[
        <li class="list-item">{{ text }}</li>
    ]=]
end

-- Main page handler using api.html for HTTP response
function api.get()
    local data = {
        user = {
            name = "Thales"
        },
        items = {
            {title = "Item 1"},
            {title = "Item 2"},
            {title = "Item 3"}
        },
        show_footer = true
    }

    return api.html(data) [=[
        <!DOCTYPE html>
        <html>
        <head>
            <title>Hello {{ user.name }}</title>
        </head>
        <body>
            <h1>Hello {{ user.name }}</h1>

            {{ card { title = "Welcome", content = "This is a card component!" } }}

            <ul>
                {{ for _, item in ipairs(items) do }}
                    {{ list_item { text = item.title } }}
                {{ end }}
            </ul>

            {{ if show_footer then }}
                <footer>
                    <p>Footer content - {{ user.name:upper() }}</p>
                </footer>
            {{ end }}
        </body>
        </html>
    ]=]
end

return api
