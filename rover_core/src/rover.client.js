// Rover Client Runtime
// Handles component events, DOM diffing, and Alpine.js integration

window.__roverComponents = window.__roverComponents || {};

// Directive transformation mapping
const ROVER_ATTR_TRANSFORMS = [
  { from: 'rover-data', to: 'x-data' },
  { from: 'rover-text', to: 'x-text' },
  { from: 'rover-show', to: 'x-show' },
  { from: 'rover-init', to: 'x-init' },
  { from: 'rover-model', to: 'x-model' }
];

function forEachWithSelf(root, selector, callback) {
  if (!root || typeof selector !== 'string') return;
  if (root.matches && root.matches(selector)) callback(root);
  if (root.querySelectorAll) root.querySelectorAll(selector).forEach(callback);
}

function transformAttributes(root) {
  ROVER_ATTR_TRANSFORMS.forEach(({ from, to }) => {
    forEachWithSelf(root, '[' + from + ']', (el) => {
      const value = el.getAttribute(from);
      if (value === null) {
        el.removeAttribute(from);
        return;
      }
      if (!el.hasAttribute(to)) {
        el.setAttribute(to, value);
      }
      el.removeAttribute(from);
    });
  });
}

function parseRoverAction(value) {
  if (!value) return null;
  const trimmed = value.trim();
  if (!trimmed.length) return null;
  const open = trimmed.indexOf('(');
  if (open === -1) return { method: trimmed, args: '' };
  const close = trimmed.lastIndexOf(')');
  if (close === -1) return { method: trimmed, args: '' };
  return {
    method: trimmed.slice(0, open).trim(),
    args: trimmed.slice(open + 1, close).trim()
  };
}

function buildRoverClickExpression(action) {
  if (!action || !action.method) return null;
  const escapedMethod = action.method.replace(/'/g, "\\'");
  const argsSegment = action.args && action.args.length ? ', ' + action.args : '';
  return "$event && $event.preventDefault && $event.preventDefault(); $rover.call('" + escapedMethod + "'" + argsSegment + ");";
}

// Transform @shorthand attributes to x-on: format (Safari compatibility)
function transformAlpineShorthands(root) {
  // Find all elements and check for @ prefixed attributes
  const allElements = root.querySelectorAll ? Array.from(root.querySelectorAll('*')) : [];
  if (root.attributes) allElements.unshift(root);
  
  allElements.forEach(el => {
    // Get all attribute names (copy to array since we'll modify)
    const attrNames = Array.from(el.attributes || []).map(a => a.name);
    attrNames.forEach(name => {
      if (name.startsWith('@')) {
        const value = el.getAttribute(name);
        const newName = 'x-on:' + name.slice(1);
        if (!el.hasAttribute(newName)) {
          el.setAttribute(newName, value);
        }
        el.removeAttribute(name);
      }
    });
  });
}

function transformRoverClicks(root) {
  forEachWithSelf(root, '[rover-click]', (el) => {
    const action = parseRoverAction(el.getAttribute('rover-click'));
    const roverExpression = buildRoverClickExpression(action);
    if (!roverExpression) {
      el.removeAttribute('rover-click');
      return;
    }
    
    // Check for existing click handlers and merge them
    const existingXOnClick = el.getAttribute('x-on:click');
    const existingAtClick = el.getAttribute('@click');
    
    if (existingXOnClick) {
      // Prepend rover call to existing x-on:click
      el.setAttribute('x-on:click', roverExpression + ' ' + existingXOnClick);
    } else if (existingAtClick) {
      // Prepend rover call to existing @click, move to x-on:click
      el.setAttribute('x-on:click', roverExpression + ' ' + existingAtClick);
      el.removeAttribute('@click');
    } else {
      // No existing handler, just set x-on:click
      el.setAttribute('x-on:click', roverExpression);
    }
    el.removeAttribute('rover-click');
  });
}

function bindLoadingElements(root) {
  forEachWithSelf(root, '[rover-loading]', (el) => {
    if (el.__roverLoadingBound) return;
    const className = (el.getAttribute('rover-loading') || 'rover-loading').trim() || 'rover-loading';
    const container = el.closest('[data-rover-component]');
    if (!container) return;
    const handler = (event) => {
      const isActive = event.detail && event.detail.active;
      if (isActive) el.classList.add(className);
      else el.classList.remove(className);
    };
    container.addEventListener('rover:loading', handler);
    el.__roverLoadingBound = true;
  });
}

function applyRoverDirectives(root) {
  const scope = root || document;
  transformAlpineShorthands(scope);  // Must come first - Safari compat
  transformAttributes(scope);
  transformRoverClicks(scope);
  bindLoadingElements(scope);
}

// Expose globally for re-application after server updates
window.__applyRoverDirectives = applyRoverDirectives;

function dispatchRoverLoading(container, active) {
  if (!container) return;
  container.dispatchEvent(new CustomEvent('rover:loading', {
    detail: { active: active }
  }));
}

function applyPatches(container, patches) {
  try {
    for (let i = 0; i < patches.length; i++) {
      const patch = patches[i];
      const el = container.querySelector(patch.selector);
      if (!el) continue;
      if (patch.type === 'replace') el.textContent = patch.text;
      else if (patch.type === 'set_attr') el.setAttribute(patch.attr, patch.value);
      else if (patch.type === 'remove_attr') el.removeAttribute(patch.attr);
      else if (patch.type === 'replace_html') el.innerHTML = patch.html;
    }
    return true;
  } catch (e) {
    console.warn('[Rover] Patch failed:', e);
    return false;
  }
}

function morphNode(fromNode, toNode) {
  if (fromNode.isEqualNode(toNode)) return;
  if (fromNode.nodeType === 3) {
    if (fromNode.nodeValue !== toNode.nodeValue) fromNode.nodeValue = toNode.nodeValue;
    return;
  }
  if (fromNode.nodeType === 1 && toNode.nodeType === 1) {
    const fromAttrs = fromNode.attributes;
    const toAttrs = toNode.attributes;
    for (let i = fromAttrs.length - 1; i >= 0; i--) {
      if (!toNode.hasAttribute(fromAttrs[i].name)) fromNode.removeAttribute(fromAttrs[i].name);
    }
    for (let j = 0; j < toAttrs.length; j++) {
      if (fromNode.getAttribute(toAttrs[j].name) !== toAttrs[j].value) {
        fromNode.setAttribute(toAttrs[j].name, toAttrs[j].value);
      }
    }
    const fromChildren = Array.prototype.slice.call(fromNode.childNodes);
    const toChildren = Array.prototype.slice.call(toNode.childNodes);
    for (let k = fromChildren.length - 1; k >= toChildren.length; k--) {
      fromNode.removeChild(fromChildren[k]);
    }
    for (let m = 0; m < toChildren.length; m++) {
      if (m >= fromChildren.length) fromNode.appendChild(toChildren[m].cloneNode(true));
      else morphNode(fromChildren[m], toChildren[m]);
    }
  }
}

function morphContainer(container, html) {
  const tempDiv = document.createElement('div');
  tempDiv.innerHTML = html;
  
  // Transform directives in the new HTML BEFORE morphing (Safari compatibility)
  applyRoverDirectives(tempDiv);
  
  const newChildren = Array.prototype.slice.call(tempDiv.childNodes);
  const oldChildren = Array.prototype.slice.call(container.childNodes);
  for (let i = oldChildren.length - 1; i >= newChildren.length; i--) {
    container.removeChild(oldChildren[i]);
  }
  for (let j = 0; j < newChildren.length; j++) {
    if (j >= oldChildren.length) container.appendChild(newChildren[j].cloneNode(true));
    else morphNode(oldChildren[j], newChildren[j]);
  }
}

async function roverEvent(event, componentId, eventName, eventData) {
  if (event && typeof event.preventDefault === 'function') event.preventDefault();

  const container = document.getElementById('rover-' + componentId);
  if (!container) {
    console.error('[Rover] Container not found:', componentId);
    return;
  }

  const component = window.__roverComponents[componentId];
  if (!component) {
    console.error('[Rover] Component not found:', componentId);
    return;
  }

  let data = eventData;
  if (data === undefined && event && event.target) {
    const target = event.target;
    if (target.type === 'checkbox') data = target.checked;
    else if (target.value !== undefined) data = target.value;
  }

  // Read Alpine's current data (includes x-model changes) before sending to server
  let stateToSend = component.state;
  const xDataEl = container.querySelector('[x-data]') || (container.hasAttribute('x-data') ? container : null);
  console.log('[Rover DEBUG] xDataEl:', xDataEl ? 'found' : 'not found');
  console.log('[Rover DEBUG] _x_dataStack:', xDataEl && xDataEl._x_dataStack);
  if (xDataEl && xDataEl._x_dataStack && xDataEl._x_dataStack[0]) {
    const alpineData = xDataEl._x_dataStack[0];
    console.log('[Rover DEBUG] Alpine data:', JSON.stringify(alpineData, (k,v) => typeof v === 'function' ? '[fn]' : v));
    
    // Build stateToSend by starting with server state, then deep merge Alpine changes
    stateToSend = JSON.parse(JSON.stringify(component.state)); // Deep clone to avoid reference issues
    for (const key in alpineData) {
      if (Object.prototype.hasOwnProperty.call(alpineData, key) && typeof alpineData[key] !== 'function') {
        const alpineValue = alpineData[key];
        const serverValue = component.state[key];
        
        // If both are objects, deep merge. Otherwise, just replace.
        if (typeof alpineValue === 'object' && alpineValue !== null && 
            typeof serverValue === 'object' && serverValue !== null &&
            !Array.isArray(alpineValue) && !Array.isArray(serverValue)) {
          // Deep merge objects (for formData, etc.)
          stateToSend[key] = JSON.parse(JSON.stringify(serverValue));
          for (const subKey in alpineValue) {
            if (Object.prototype.hasOwnProperty.call(alpineValue, subKey)) {
              stateToSend[key][subKey] = alpineValue[subKey];
            }
          }
        } else {
          // Replace non-object values
          stateToSend[key] = alpineValue;
        }
      }
    }
    console.log('[Rover DEBUG] stateToSend:', JSON.stringify(stateToSend));
  } else {
    console.log('[Rover DEBUG] No Alpine data found, using component.state');
  }

  const evtTarget = event ? event.target : null;
  const originalCursor = container.style.cursor;
  const originalOpacity = container.style.opacity;
  const isButton = evtTarget && evtTarget.tagName === 'BUTTON';
  const prevDisabled = isButton ? evtTarget.disabled : undefined;

  container.classList.add('rover-loading');
  container.style.cursor = 'wait';
  container.style.opacity = '0.7';
  if (isButton) evtTarget.disabled = true;
  dispatchRoverLoading(container, true);

  try {
    const response = await fetch('/__rover/component-event', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        instanceId: componentId,
        eventName: eventName,
        state: stateToSend,
        data: data
      })
    });

    if (!response.ok) throw new Error('Component event failed: ' + response.statusText);

    const result = await response.json();
    component.state = result.state;
    container.dataset.roverState = JSON.stringify(result.state);

    // Destroy Alpine on the container before morphing to avoid stale bindings
    if (window.Alpine && window.Alpine.destroyTree) {
      window.Alpine.destroyTree(container);
    }

    if (result.patches && result.patches.length > 0) {
      const ok = applyPatches(container, result.patches);
      if (!ok && result.html && result.html.length > 0) morphContainer(container, result.html);
    } else if (result.html && result.html.length > 0) {
      morphContainer(container, result.html);
    }

    // Re-apply rover directives on updated DOM
    if (typeof window.__applyRoverDirectives === 'function') {
      window.__applyRoverDirectives(container);
    }

    // Re-initialize Alpine on the updated DOM
    if (window.Alpine && window.Alpine.initTree) {
      window.Alpine.initTree(container);
    }
  } catch (error) {
    console.error('[Rover] Event error:', error);
    container.style.border = '2px solid #f44336';
    setTimeout(function() { container.style.border = ''; }, 2000);
  } finally {
    container.classList.remove('rover-loading');
    container.style.cursor = originalCursor;
    container.style.opacity = originalOpacity;
    if (isButton && evtTarget) evtTarget.disabled = prevDisabled;
    dispatchRoverLoading(container, false);
  }
}

// Setup Alpine.js integration
// This runs BEFORE alpine:init because we set it up in advance
document.addEventListener('alpine:init', () => {
  const Alpine = window.Alpine;
  if (!Alpine) return;

  // Register $rover magic
  Alpine.magic('rover', (el) => {
    const container = el.closest('[data-rover-component]');
    return {
      call(method, ...args) {
        if (!container || !method) return;
        const componentId = container.dataset.roverComponent;
        if (!componentId) return;
        const payload = args.length === 0 ? undefined : args.length === 1 ? args[0] : args;
        roverEvent({ preventDefault: function(){} }, componentId, method, payload);
      },
      get state() {
        if (!container || !container.dataset.roverState) return {};
        try {
          return JSON.parse(container.dataset.roverState);
        } catch (e) {
          console.warn('[Rover] Failed to parse state', e);
          return {};
        }
      },
      get loading() {
        return !!(container && container.classList.contains('rover-loading'));
      }
    };
  });
});

// Transform directives IMMEDIATELY when script loads
// This must run BEFORE Alpine.js evaluates any directives
applyRoverDirectives(document);

// Initialize Alpine on the component container if Alpine is already loaded
// This ensures data scopes are set up before event handlers are evaluated
if (window.Alpine && window.Alpine.initTree) {
  const container = document.querySelector('[data-rover-component]');
  if (container) {
    window.Alpine.initTree(container);
  }
}
