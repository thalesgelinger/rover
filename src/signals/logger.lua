local logger = {}

logger.log = function(msg)
    local logfile = io.open("log.log", "a")
    logfile:write(msg)
    logfile:close()
end

return logger
