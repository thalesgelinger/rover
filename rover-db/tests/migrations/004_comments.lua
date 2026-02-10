-- Create comments table
function change()
    migration.comments:create({body = rover.db.guard:string()})
end
