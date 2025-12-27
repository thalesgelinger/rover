local api = rover.server {}

-- Serve HTML response instead of JSON
function api.get()

    local data = {
        user = {
            name= "Thales"
        },
        items = {
            {title = "Title"}
        }
    }

    return api.html(data) [[ 
      <h1>Hello {{ user.name }}</h1>

      <ul>
        {{ for _, item in ipairs(items) do }}
          <li>{{ item.title }}</li>
        {{ end }}
      </ul>
    ]]
end

return api
