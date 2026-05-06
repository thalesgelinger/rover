// Static Assets Example - Application JavaScript

(function() {
    'use strict';

    // Track page loads to demonstrate 304 Not Modified behavior
    function trackPageLoads() {
        const loadCountElement = document.getElementById('load-count');
        if (!loadCountElement) return;

        // Get current count from sessionStorage (persists for session)
        let count = parseInt(sessionStorage.getItem('pageLoads') || '0', 10);
        count++;
        sessionStorage.setItem('pageLoads', count.toString());

        loadCountElement.textContent = `Page loads this session: ${count}`;

        // Add reload button
        const reloadBtn = document.createElement('button');
        reloadBtn.textContent = 'Reload to test 304';
        reloadBtn.style.cssText = 'margin-left: 1rem; padding: 0.5rem 1rem; cursor: pointer;';
        reloadBtn.onclick = function() {
            window.location.reload();
        };
        loadCountElement.appendChild(reloadBtn);
    }

    // Log cache information (visible in browser console)
    function logCacheInfo() {
        console.log('%cStatic Assets Example', 'font-size: 20px; font-weight: bold; color: #2563eb;');
        console.log('Check the Network tab to observe:');
        console.log('  1. First request: 200 OK with full response');
        console.log('  2. Subsequent requests: 304 Not Modified');
        console.log('  3. Response headers: ETag, Last-Modified, Cache-Control');
    }

    // Initialize when DOM is ready
    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', function() {
            trackPageLoads();
            logCacheInfo();
        });
    } else {
        trackPageLoads();
        logCacheInfo();
    }
})();
