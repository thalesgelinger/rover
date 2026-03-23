// Uploads Demo - Client-side JavaScript

(function() {
    'use strict';

    const API_BASE = '';

    // Show message to user
    function showMessage(text, type) {
        const msgEl = document.getElementById('message');
        if (!msgEl) return;

        msgEl.textContent = text;
        msgEl.className = 'message ' + type;
        msgEl.style.display = 'block';

        setTimeout(() => {
            msgEl.style.display = 'none';
        }, 5000);
    }

    // Load and display uploaded files
    async function loadFiles() {
        const container = document.getElementById('files-container');
        if (!container) return;

        try {
            const response = await fetch(API_BASE + '/uploads');
            const data = await response.json();

            if (data.files && data.files.length > 0) {
                const list = document.createElement('ul');
                list.className = 'uploaded-files';

                data.files.forEach(file => {
                    const li = document.createElement('li');
                    li.innerHTML = `
                        <div class="file-info">
                            <span>${escapeHtml(file.filename)}</span>
                            <span class="file-size">(${formatBytes(file.size)})</span>
                        </div>
                        <div class="file-actions">
                            <a href="${file.url}" target="_blank" download>Download</a>
                            <button class="delete" data-filename="${escapeHtml(file.filename)}">Delete</button>
                        </div>
                    `;
                    list.appendChild(li);
                });

                container.innerHTML = '';
                container.appendChild(list);

                // Add delete handlers
                container.querySelectorAll('button.delete').forEach(btn => {
                    btn.addEventListener('click', () => deleteFile(btn.dataset.filename));
                });
            } else {
                container.innerHTML = '<div class="empty-state">No files uploaded yet</div>';
            }
        } catch (error) {
            container.innerHTML = '<div class="empty-state">Error loading files</div>';
            console.error('Failed to load files:', error);
        }
    }

    // Upload file
    async function uploadFile(formData) {
        try {
            const response = await fetch(API_BASE + '/uploads', {
                method: 'POST',
                body: formData
            });

            const data = await response.json();

            if (response.ok) {
                showMessage('File uploaded successfully!', 'success');
                loadFiles();
                return true;
            } else {
                showMessage(data.error || 'Upload failed', 'error');
                return false;
            }
        } catch (error) {
            showMessage('Upload failed: ' + error.message, 'error');
            return false;
        }
    }

    // Delete file
    async function deleteFile(filename) {
        if (!confirm('Are you sure you want to delete "' + filename + '"?')) {
            return;
        }

        try {
            const response = await fetch(API_BASE + '/uploads/' + encodeURIComponent(filename), {
                method: 'DELETE'
            });

            const data = await response.json();

            if (response.ok) {
                showMessage('File deleted successfully', 'success');
                loadFiles();
            } else {
                showMessage(data.error || 'Delete failed', 'error');
            }
        } catch (error) {
            showMessage('Delete failed: ' + error.message, 'error');
        }
    }

    // Utility: Format bytes
    function formatBytes(bytes) {
        if (bytes === 0) return '0 Bytes';
        const k = 1024;
        const sizes = ['Bytes', 'KB', 'MB', 'GB'];
        const i = Math.floor(Math.log(bytes) / Math.log(k));
        return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
    }

    // Utility: Escape HTML
    function escapeHtml(text) {
        const div = document.createElement('div');
        div.textContent = text;
        return div.innerHTML;
    }

    // Initialize
    function init() {
        const form = document.getElementById('upload-form');
        if (form) {
            form.addEventListener('submit', async (e) => {
                e.preventDefault();
                const formData = new FormData(form);
                await uploadFile(formData);
                form.reset();
            });
        }

        // Load files on page load
        loadFiles();
    }

    // Run when DOM is ready
    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', init);
    } else {
        init();
    }
})();
