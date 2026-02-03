-- Create comments table
function change()
    migration.comments:create({body = rover.guard:string()})
end
