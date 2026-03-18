-- Example: Session Management Demo
-- This demonstrates the rover.session module for secure cookie sessions

local api = rover.server {}

-- Create a session store with custom configuration
local store = rover.session.new {
    cookie_name = "demo_session",
    ttl = 3600,           -- 1 hour
    secure = true,
    http_only = true,
    same_site = "lax",
    path = "/",
}

-- Route to set session data
function api.session_set.post(ctx)
    local body = ctx:body():json()
    local key = body.key
    local value = body.value
    
    -- Get session ID from cookie or create new session
    local session_id = nil  -- In real use, extract from request cookie
    local session = store:get_or_create(session_id)
    
    -- Set session data
    session:set(key, value)
    session:save()
    
    -- Return the session cookie for the client to store
    return {
        ok = true,
        session_id = session:id(),
        cookie = session:cookie(),
    }
end

-- Route to get session data
function api.session_get.p_id.get(ctx)
    local session_id = ctx.params.id
    local session = store:get(session_id)
    
    if not session then
        return api:error(404, "Session not found")
    end
    
    -- Get all session data
    local data = {}
    -- Note: In production, you'd iterate over session keys
    
    return {
        session_id = session:id(),
        created_at = session:created_at(),
        last_accessed = session:last_accessed(),
        is_empty = session:is_empty(),
        len = session:len(),
    }
end

-- Route to destroy session
function api.session_destroy.p_id.post(ctx)
    local session_id = ctx.params.id
    local session = store:get(session_id)
    
    if not session then
        return api:error(404, "Session not found")
    end
    
    session:destroy()
    
    return {
        ok = true,
        message = "Session destroyed",
    }
end

-- Route to regenerate session ID (security best practice after login)
function api.session_regenerate.p_id.post(ctx)
    local session_id = ctx.params.id
    local session = store:get(session_id)
    
    if not session then
        return api:error(404, "Session not found")
    end
    
    local new_id = session:regenerate()
    
    return {
        ok = true,
        old_id = session_id,
        new_id = new_id,
        cookie = session:cookie(),
    }
end

return api
