local app = rover.server {}

app.use "blueprint.features.posts.api"
app.use "blueprint/features/posts/view.html.lua"

return app
