-- Rover Migration DSL
-- Provides migration.<table>:operation() syntax and change/up/down functions

local MigrationDSL = {}

-- Current migration being executed
local current_migration = nil
local migration_operations = {}

-- Record operations for change() auto-reverse
function MigrationDSL.record_operation(operation)
    table.insert(migration_operations, operation)
end

function MigrationDSL.get_operations()
    return migration_operations
end

function MigrationDSL.clear_operations()
    migration_operations = {}
end

-- Table operations context
local function create_table_context(table_name)
    return {
        _table_name = table_name,

        create = function(self, definition)
            MigrationDSL.record_operation({
                type = "create_table",
                table = table_name,
                definition = definition,
            })
            return self
        end,

        drop = function(self)
            MigrationDSL.record_operation({
                type = "drop_table",
                table = table_name,
            })
            return self
        end,

        add_column = function(self, column_name, column_type)
            MigrationDSL.record_operation({
                type = "add_column",
                table = table_name,
                column = column_name,
                column_type = column_type,
            })
            return self
        end,

        remove_column = function(self, column_name)
            MigrationDSL.record_operation({
                type = "remove_column",
                table = table_name,
                column = column_name,
            })
            return self
        end,

        rename_column = function(self, old_name, new_name)
            MigrationDSL.record_operation({
                type = "rename_column",
                table = table_name,
                old_column = old_name,
                new_column = new_name,
            })
            return self
        end,

        create_index = function(self, index_name, columns)
            MigrationDSL.record_operation({
                type = "create_index",
                table = table_name,
                index = index_name,
                columns = columns,
            })
            return self
        end,

        drop_index = function(self, index_name)
            MigrationDSL.record_operation({
                type = "drop_index",
                table = table_name,
                index = index_name,
            })
            return self
        end,

        rename = function(self, new_name)
            MigrationDSL.record_operation({
                type = "rename_table",
                table = table_name,
                new_table = new_name,
            })
            return self
        end,
    }
end

-- Metatable for dynamic table access: migration.<table>
local migration_mt = {
    __index = function(self, table_name)
        return create_table_context(table_name)
    end,
}

setmetatable(MigrationDSL, migration_mt)

-- Raw SQL escape hatch
function MigrationDSL.raw(sql)
    MigrationDSL.record_operation({
        type = "raw",
        sql = sql,
    })
end

-- Helper to execute change/up/down functions
function MigrationDSL.execute_migration_function(migration_fn_name, migration_fn)
    MigrationDSL.clear_operations()
    if migration_fn then
        migration_fn()
    end
    return MigrationDSL.get_operations()
end

return MigrationDSL
