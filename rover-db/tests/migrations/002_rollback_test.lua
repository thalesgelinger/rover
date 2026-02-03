-- Create users table for rollback test
function change()
    migration.users:create({
        name = rover.guard:string():required(),
    })
end
