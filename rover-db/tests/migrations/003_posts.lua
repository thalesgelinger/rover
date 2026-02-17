-- Create posts table
function change()
    migration.posts:create({title = rover.db.guard:string()})
end
