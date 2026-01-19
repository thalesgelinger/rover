
migration.users:create({
    name = rover.guard:string():required(),
    email = rover.guard:string(),
})
