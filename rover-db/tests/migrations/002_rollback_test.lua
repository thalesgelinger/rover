-- Create users table for rollback test
function change()
    migration.users:create({
        name = rover.db.guard:string():required(),
    })
end
