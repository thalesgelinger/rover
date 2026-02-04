-- Create users table
function change()
    migration.users:create({
        name = rover.db.guard:string():required(),
        email = rover.db.guard:string(),
    })
end
