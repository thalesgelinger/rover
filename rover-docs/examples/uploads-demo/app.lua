-- Uploads Demo Example for rover-docs
-- Demonstrates multipart upload handling and static file serving for uploads

local api = rover.server {}

-- ============================================================================
-- Static Mount for Uploaded Files
-- ============================================================================

-- Serve uploaded files from the 'uploads/' directory
-- User uploads typically should NOT be cached aggressively
api.uploads.static {
    dir = "uploads",
    cache = "private, max-age=0, must-revalidate",  -- No caching for user uploads
}

-- ============================================================================
-- Main Application Routes
-- ============================================================================

-- Serve an HTML page with upload form
function api.get(ctx)
    return api.html {
        title = "Uploads Demo",
    } [[
        <!DOCTYPE html>
        <html lang="en">
        <head>
            <meta charset="UTF-8">
            <meta name="viewport" content="width=device-width, initial-scale=1.0">
            <title>{{ title }}</title>
            <style>
                :root {
                    --primary-color: #2563eb;
                    --primary-hover: #1d4ed8;
                    --bg-color: #f8fafc;
                    --card-bg: #ffffff;
                    --text-color: #1e293b;
                    --text-secondary: #64748b;
                    --border-color: #e2e8f0;
                    --error-bg: #fef2f2;
                    --error-color: #dc2626;
                    --success-bg: #f0fdf4;
                    --success-color: #16a34a;
                }

                * {
                    margin: 0;
                    padding: 0;
                    box-sizing: border-box;
                }

                body {
                    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
                    background-color: var(--bg-color);
                    color: var(--text-color);
                    line-height: 1.6;
                }

                .container {
                    max-width: 800px;
                    margin: 0 auto;
                    padding: 2rem 1rem;
                }

                header {
                    text-align: center;
                    margin-bottom: 2rem;
                }

                header h1 {
                    font-size: 2rem;
                    margin-bottom: 0.5rem;
                    color: var(--primary-color);
                }

                .subtitle {
                    color: var(--text-secondary);
                }

                .card {
                    background: var(--card-bg);
                    border-radius: 0.5rem;
                    padding: 1.5rem;
                    margin-bottom: 1.5rem;
                    box-shadow: 0 1px 3px 0 rgb(0 0 0 / 0.1);
                    border: 1px solid var(--border-color);
                }

                .card h2 {
                    font-size: 1.25rem;
                    margin-bottom: 1rem;
                }

                .form-group {
                    margin-bottom: 1rem;
                }

                label {
                    display: block;
                    margin-bottom: 0.5rem;
                    font-weight: 500;
                }

                input[type="text"],
                input[type="file"] {
                    width: 100%;
                    padding: 0.5rem;
                    border: 1px solid var(--border-color);
                    border-radius: 0.25rem;
                    font-size: 1rem;
                }

                input[type="file"] {
                    padding: 0.75rem;
                    background-color: var(--bg-color);
                }

                button {
                    background-color: var(--primary-color);
                    color: white;
                    padding: 0.75rem 1.5rem;
                    border: none;
                    border-radius: 0.25rem;
                    font-size: 1rem;
                    cursor: pointer;
                    font-weight: 500;
                }

                button:hover {
                    background-color: var(--primary-hover);
                }

                button:disabled {
                    opacity: 0.6;
                    cursor: not-allowed;
                }

                .message {
                    padding: 1rem;
                    border-radius: 0.25rem;
                    margin-bottom: 1rem;
                    display: none;
                }

                .message.error {
                    background-color: var(--error-bg);
                    color: var(--error-color);
                    border: 1px solid var(--error-color);
                }

                .message.success {
                    background-color: var(--success-bg);
                    color: var(--success-color);
                    border: 1px solid var(--success-color);
                }

                .uploaded-files {
                    list-style: none;
                    margin: 0;
                    padding: 0;
                }

                .uploaded-files li {
                    display: flex;
                    justify-content: space-between;
                    align-items: center;
                    padding: 0.75rem;
                    border-bottom: 1px solid var(--border-color);
                }

                .uploaded-files li:last-child {
                    border-bottom: none;
                }

                .file-info {
                    display: flex;
                    align-items: center;
                    gap: 0.5rem;
                }

                .file-size {
                    color: var(--text-secondary);
                    font-size: 0.875rem;
                }

                .file-actions a {
                    color: var(--primary-color);
                    text-decoration: none;
                    margin-right: 1rem;
                }

                .file-actions a:hover {
                    text-decoration: underline;
                }

                .file-actions button.delete {
                    background: transparent;
                    color: var(--error-color);
                    padding: 0.25rem 0.5rem;
                    font-size: 0.875rem;
                }

                code {
                    font-family: "Menlo", "Monaco", "Courier New", monospace;
                    background-color: #f1f5f9;
                    padding: 0.2rem 0.4rem;
                    border-radius: 0.25rem;
                    font-size: 0.875em;
                }

                .empty-state {
                    text-align: center;
                    color: var(--text-secondary);
                    padding: 2rem;
                }

                footer {
                    text-align: center;
                    margin-top: 2rem;
                    padding-top: 2rem;
                    border-top: 1px solid var(--border-color);
                    color: var(--text-secondary);
                }

                footer a {
                    color: var(--primary-color);
                    text-decoration: none;
                }
            </style>
        </head>
        <body>
            <div class="container">
                <header>
                    <h1>Uploads Demo</h1>
                    <p class="subtitle">Multipart upload handling with static file serving</p>
                </header>

                <div id="message" class="message"></div>

                <section class="card">
                    <h2>Upload File</h2>
                    <form id="upload-form" enctype="multipart/form-data">
                        <div class="form-group">
                            <label for="description">Description (optional)</label>
                            <input type="text" id="description" name="description" placeholder="Enter file description">
                        </div>

                        <div class="form-group">
                            <label for="file">Select File</label>
                            <input type="file" id="file" name="file" required>
                        </div>

                        <button type="submit">Upload File</button>
                    </form>
                </section>

                <section class="card">
                    <h2>Uploaded Files</h2>
                    <div id="files-container">
                        <div class="empty-state">Loading files...</div>
                    </div>
                </section>

                <section class="card">
                    <h2>API Endpoints</h2>
                    <p>This example demonstrates the following endpoints:</p>
                    <ul style="margin-left: 1.5rem; margin-top: 0.5rem;">
                        <li><code>POST /uploads</code> - Upload a file (multipart/form-data)</li>
                        <li><code>GET /uploads</code> - List all uploaded files</li>
                        <li><code>GET /uploads/:filename</code> - Download a file (static)</li>
                        <li><code>GET /uploads/:filename/meta</code> - Get file metadata</li>
                        <li><code>DELETE /uploads/:filename</code> - Delete a file</li>
                    </ul>
                </section>

                <footer>
                    <p>Part of <a href="https://github.com/thalesgelinger/rover">rover-docs</a> examples</p>
                </footer>
            </div>

            <script src="/app.js"></script>
        </body>
        </html>
    ]]
end

-- ============================================================================
-- Multipart Upload Endpoint
-- ============================================================================

-- Handle multipart file uploads
-- Uses ctx:body():file() to extract uploaded files
function api.uploads.post(ctx)
    -- Extract the uploaded file
    local file = ctx:body():file("file")

    if not file then
        return api:error(400, "No file provided. Use multipart/form-data with a 'file' field.")
    end

    -- Validate filename (prevent path traversal)
    local filename = file.name:gsub("[\\/]", "_")
    if filename:match("^%.") or filename == "" then
        return api:error(400, "Invalid filename")
    end

    -- Validate file type (basic check)
    local allowed_types = {
        ["image/jpeg"] = true,
        ["image/png"] = true,
        ["image/gif"] = true,
        ["image/webp"] = true,
        ["text/plain"] = true,
        ["text/markdown"] = true,
        ["application/pdf"] = true,
    }

    if not allowed_types[file.type] then
        return api:error(400, "File type not allowed: " .. (file.type or "unknown"))
    end

    -- Validate file size (max 10MB)
    local max_size = 10 * 1024 * 1024  -- 10MB
    if file.size > max_size then
        return api:error(400, "File too large. Maximum size is 10MB.")
    end

    local filepath = string.format("uploads/%s", filename)

    -- Check if file already exists
    local existing = io.open(filepath, "rb")
    if existing then
        existing:close()
        return api:error(409, "File already exists: " .. filename)
    end

    -- Write file to disk
    local f = io.open(filepath, "wb")
    if not f then
        return api:error(500, "Failed to save file")
    end

    f:write(file.data)
    f:close()

    -- Get form data (description)
    local form = ctx:body():form()

    return api.json:status(201, {
        message = "File uploaded successfully",
        filename = filename,
        original_name = file.name,
        size = file.size,
        type = file.type,
        description = form.description,
        url = string.format("/uploads/%s", filename),
        meta_url = string.format("/uploads/%s/meta", filename),
    })
end

-- ============================================================================
-- File Listing Endpoint
-- ============================================================================

-- List all uploaded files with metadata
function api.uploads.get(ctx)
    local files = {}

    -- Simple directory listing using io.popen
    -- In production, use a database to track uploads
    local handle = io.popen("ls -la uploads/ 2>/dev/null || echo ''")
    if handle then
        for line in handle:lines() do
            -- Parse ls -la output: -rw-r--r-- 1 user group 12345 Jan 01 12:34 filename
            local size, name = line:match("^[%w%-]+%s+%d+%s+%S+%s+%S+%s+(%d+)%s+%w+%s+%d+%s+[%d:]+%s+(.+)$")
            if size and name and not name:match("^%.") then
                -- Get file info
                local filepath = string.format("uploads/%s", name)
                local attr = io.open(filepath, "rb")
                if attr then
                    local content = attr:read("*a")
                    attr:close()

                    -- Detect content type (simplified)
                    local content_type = "application/octet-stream"
                    if name:match("%.jpg$") or name:match("%.jpeg$") then
                        content_type = "image/jpeg"
                    elseif name:match("%.png$") then
                        content_type = "image/png"
                    elseif name:match("%.gif$") then
                        content_type = "image/gif"
                    elseif name:match("%.txt$") then
                        content_type = "text/plain"
                    elseif name:match("%.pdf$") then
                        content_type = "application/pdf"
                    end

                    table.insert(files, {
                        filename = name,
                        size = tonumber(size),
                        content_type = content_type,
                        url = string.format("/uploads/%s", name),
                        meta_url = string.format("/uploads/%s/meta", name),
                    })
                end
            end
        end
        handle:close()
    end

    -- Sort by filename
    table.sort(files, function(a, b) return a.filename < b.filename end)

    return {
        files = files,
        count = #files,
    }
end

-- ============================================================================
-- File Metadata Endpoint
-- ============================================================================

-- Get metadata for a specific uploaded file (takes precedence over static mount)
function api.uploads.p_filename.get(ctx)
    local filename = ctx:params().filename:gsub("[\\/]", "_")
    local filepath = string.format("uploads/%s", filename)

    -- Check if file exists
    local f = io.open(filepath, "rb")
    if not f then
        return api:error(404, "File not found")
    end

    local content = f:read("*a")
    f:close()

    -- Detect content type
    local content_type = "application/octet-stream"
    if filename:match("%.jpg$") or filename:match("%.jpeg$") then
        content_type = "image/jpeg"
    elseif filename:match("%.png$") then
        content_type = "image/png"
    elseif filename:match("%.gif$") then
        content_type = "image/gif"
    elseif filename:match("%.txt$") then
        content_type = "text/plain"
    elseif filename:match("%.pdf$") then
        content_type = "application/pdf"
    end

    return {
        filename = filename,
        size = #content,
        content_type = content_type,
        url = string.format("/uploads/%s", filename),
        download_url = string.format("/uploads/%s", filename),
    }
end

-- ============================================================================
-- File Delete Endpoint
-- ============================================================================

-- Delete an uploaded file
function api.uploads.p_filename.delete(ctx)
    local filename = ctx:params().filename:gsub("[\\/]", "_")
    local filepath = string.format("uploads/%s", filename)

    -- Check if file exists
    local f = io.open(filepath, "rb")
    if not f then
        return api:error(404, "File not found")
    end
    f:close()

    -- Delete the file
    local ok, err = os.remove(filepath)
    if not ok then
        return api:error(500, "Failed to delete file: " .. (err or "unknown error"))
    end

    return api.json:status(200, {
        message = "File deleted successfully",
        filename = filename,
    })
end

return api
