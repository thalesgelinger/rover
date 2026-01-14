-- Rover Guard - Error handling utilities
local ErrorHandler = {}

-- Check if an error is an application error (ValidationErrors) vs runtime error
function ErrorHandler.is_app_error(err)
	-- ValidationErrors will be userdata with a specific type
	if type(err) == "userdata" then
		return true -- ValidationErrors are application errors
	end
	return false
end

-- Clean up error message - remove stack traces for app errors
function ErrorHandler.clean_error(err)
	if ErrorHandler.is_app_error(err) then
		-- For ValidationErrors, just convert to string (already formatted nicely)
		return tostring(err)
	else
		-- For runtime errors, keep some info but clean it up
		local err_str = tostring(err)
		-- Remove "runtime error: " prefix
		err_str = err_str:gsub("^runtime error: ", "")
		-- Remove stack traceback for production
		local stack_pos = err_str:find "\nstack traceback:"
		if stack_pos then
			err_str = err_str:sub(1, stack_pos - 1)
		end
		return err_str
	end
end

-- Smart error handling wrapper
-- Usage: ErrorHandler.handle(pcall_result_success, pcall_result_value, api, status_code)
function ErrorHandler.handle(success, result, api, status)
	if success then
		return result -- Pass through successful results
	end

	-- Check if it's an application error or runtime error
	if ErrorHandler.is_app_error(result) then
		-- Application error - clean response
		return api:error(status or 400, result) -- api:error handles conversion
	else
		-- Runtime error - this is a bug!
		-- In development: return detailed error
		-- In production: you might want to log and return generic error
		error(result) -- Re-throw to crash (dev errors should crash)
	end
end

return ErrorHandler
