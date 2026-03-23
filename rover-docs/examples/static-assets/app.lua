-- Static Assets Example for rover-docs
-- Demonstrates proper static file serving DSL and route patterns

local api = rover.server {}

-- ============================================================================
-- Static Mount Configuration
-- ============================================================================

-- Serve static files from the 'public/assets/' directory at '/assets/*' URL path
-- The 'dir' parameter is required and specifies the filesystem path
-- The 'cache' parameter is optional and sets Cache-Control header
api.assets.static {
    dir = "public/assets",
    cache = "public, max-age=31536000, immutable",  -- 1 year cache for versioned assets
}

-- ============================================================================
-- Route Precedence Demonstration
-- ============================================================================

-- API routes take precedence over static mounts at the same path
-- GET /assets/health returns API response, not a file from public/assets/health
function api.assets.health.get(ctx)
    return {
        status = "healthy",
        static_mount = "/assets/*",
        message = "API route takes precedence over static file",
        timestamp = os.time(),
    }
end

-- Dynamic route also takes precedence over static mount
-- GET /assets/info/:id returns metadata about the request
function api.assets.p_id.get(ctx)
    return {
        file_id = ctx:params().id,
        requested_path = ctx.path,
        note = "Dynamic route takes precedence over static mount",
    }
end

-- ============================================================================
-- Main Application Routes
-- ============================================================================

-- Serve an HTML page that references static assets
function api.get(ctx)
    return api.html {
        title = "Static Assets Example",
    } [[
        <!DOCTYPE html>
        <html lang="en">
        <head>
            <meta charset="UTF-8">
            <meta name="viewport" content="width=device-width, initial-scale=1.0">
            <title>{{ title }}</title>
            <link rel="stylesheet" href="/assets/site.css">
        </head>
        <body>
            <div class="container">
                <header>
                    <h1>Rover Static Assets Example</h1>
                    <p class="subtitle">Demonstrating proper DSL and static file serving</p>
                </header>

                <main>
                    <section class="card">
                        <h2>Static File Serving</h2>
                        <p>This page uses static assets served from <code>public/assets/</code>:</p>
                        <ul>
                            <li>CSS: <code>/assets/site.css</code></li>
                            <li>JavaScript: <code>/assets/app.js</code> (loaded at bottom)</li>
                        </ul>
                        <p>Check the browser's Network tab to see caching headers in action:</p>
                        <ul>
                            <li><strong>ETag</strong>: Content hash for conditional requests</li>
                            <li><strong>Last-Modified</strong>: File modification timestamp</li>
                            <li><strong>Cache-Control</strong>: Based on file extension or explicit setting</li>
                        </ul>
                    </section>

                    <section class="card">
                        <h2>Route Precedence</h2>
                        <p>API routes take precedence over static files:</p>
                        <ul>
                            <li><a href="/assets/health">/assets/health</a> → API response (not a file)</li>
                            <li><a href="/assets/info/123">/assets/info/:id</a> → Dynamic route</li>
                            <li><a href="/assets/site.css">/assets/site.css</a> → Static file</li>
                        </ul>
                    </section>

                    <section class="card">
                        <h2>304 Not Modified Demo</h2>
                        <p>Reload this page and check the Network tab:</p>
                        <ul>
                            <li>First request: <strong>200 OK</strong> with full response</li>
                            <li>Subsequent requests: <strong>304 Not Modified</strong> (no body)</li>
                        </ul>
                        <p id="load-count">Page loads: calculating...</p>
                    </section>
                </main>

                <footer>
                    <p>Part of <a href="https://github.com/thalesgelinger/rover">rover-docs</a> examples</p>
                </footer>
            </div>

            <script src="/assets/app.js"></script>
        </body>
        </html>
    ]]
end

-- ============================================================================
-- API Endpoints
-- ============================================================================

-- Return information about the static mount configuration
function api.config.get(ctx)
    return {
        static_mounts = {
            {
                path = "/assets/*",
                directory = "public/assets",
                cache_control = "public, max-age=31536000, immutable",
            },
        },
        features = {
            etag = true,
            last_modified = true,
            conditional_requests = true,
            path_traversal_protection = true,
        },
    }
end

return api
