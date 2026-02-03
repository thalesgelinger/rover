-- Create posts table
function change()
    migration.posts:create({title = rover.guard:string()})
end
