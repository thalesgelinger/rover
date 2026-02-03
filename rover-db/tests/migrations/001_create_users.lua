-- Create users table
function change()
    migration.users:create({
        name = rover.guard:string():required(),
        email = rover.guard:string(),
    })
end
