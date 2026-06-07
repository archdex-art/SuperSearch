/**
 * SuperSearch — Application Controller
 *
 * Main orchestrator that wires all modules together:
 * - Bridge (Tauri IPC)
 * - Search (query engine)
 * - Keyboard (navigation)
 * - Preview (detail panel)
 * - Agent (AI console)
 * - Telemetry (kernel metrics)
 */

import { Bridge } from './bridge.js';
import * as Search from './search.js';
import * as Keyboard from './keyboard.js';
import * as Preview from './preview.js';
import * as Agent from './agent.js';
import * as Extensions from './extensions.js';
import * as Settings from './settings.js';

/** @type {number} Currently selected result index */
let selectedIndex = -1;

/** @type {SearchResult[]} Current results */
let currentResults = [];

/** @type {boolean} Whether the preview panel is visible (off by default — Raycast-style full-width results) */
let previewVisible = false;

/** @type {number|null} Telemetry polling interval */
let telemetryInterval = null;

/** @type {boolean} Whether agent mode is active */
let agentMode = false;

/**
 * Initialize the application.
 */
export function init() {
  // DOM references
  const searchInput = document.getElementById('search-input');
  const resultsPanel = document.getElementById('results-panel');
  const previewPanel = document.getElementById('preview-panel');
  const contentArea = document.getElementById('content-area');
  const agentPanel = document.getElementById('agent-panel');
  const modeBadge = document.getElementById('mode-badge');

  if (!searchInput || !resultsPanel || !previewPanel || !contentArea || !agentPanel) {
    console.error('[App] Required DOM elements not found');
    return;
  }

  // Raycast-style full-width single column by default; Tab reveals the preview.
  contentArea.classList.add('no-preview');
  previewPanel.style.display = 'none';

  // Initialize modules
  Preview.init(previewPanel);
  Agent.init(agentPanel);
  Extensions.init();
  Settings.init();
  Keyboard.init();

  // Wire search input
  const paletteContainer = document.getElementById('palette-container');
  searchInput.addEventListener('input', (e) => {
    const query = e.target.value;

    if (query.trim().length === 0) {
      if (paletteContainer) paletteContainer.style.display = 'none';
    } else {
      if (paletteContainer) paletteContainer.style.display = 'flex';
    }

    // Hide agent panel when user types
    if (agentMode) {
      agentMode = false;
      agentPanel.classList.remove('active');
      contentArea.style.display = '';
      if (modeBadge) modeBadge.textContent = '⌥ Space';
    }

    Search.search(query);
  });

  // Wire search results
  Search.onResults((results) => {
    // Order into Raycast-style contiguous category groups (so visual order ==
    // selection order), highest-score within each group.
    currentResults = orderResults(results);
    selectedIndex = currentResults.length > 0 ? 0 : -1;
    renderResults(resultsPanel);
    updatePreview();
  });

  // Wire keyboard navigation
  Keyboard.registerCallbacks({
    onNavigate: (direction) => {
      if (Extensions.isOpen() || Settings.isOpen()) return;
      if (currentResults.length === 0) return;
      selectedIndex = Math.max(0, Math.min(currentResults.length - 1, selectedIndex + direction));
      // Glide the highlight (no list rebuild) for a smooth, Spotlight-like move.
      positionHighlight(resultsPanel, true);
      updatePreview();
    },

    onExecute: (withMeta) => {
      if (Extensions.isOpen() || Settings.isOpen()) return;
      if (selectedIndex >= 0 && selectedIndex < currentResults.length) {
        const result = currentResults[selectedIndex];
        executeResult(result, withMeta, contentArea, agentPanel, modeBadge);
      }
    },

    onDismiss: () => {
      // Overlays intercept Esc first.
      if (Settings.isOpen()) {
        Settings.close();
        return;
      }
      if (Extensions.isOpen()) {
        Extensions.close();
        return;
      }
      if (agentMode) {
        // Exit agent mode first
        agentMode = false;
        Agent.hide();
        contentArea.style.display = '';
        if (modeBadge) modeBadge.textContent = '⌥ Space';
        return;
      }
      Bridge.invoke('hide_window').catch((error) => {
        console.error('[App] Hide window failed:', error);
      });
    },

    onTogglePreview: () => {
      previewVisible = !previewVisible;
      contentArea.classList.toggle('no-preview', !previewVisible);
      previewPanel.style.display = previewVisible ? '' : 'none';
    },
  });

  // Composer toolbar actions
  const submit = () => {
    if (selectedIndex >= 0 && selectedIndex < currentResults.length) {
      executeResult(currentResults[selectedIndex], false, contentArea, agentPanel, modeBadge);
    } else if (searchInput.value.trim()) {
      enterAgent(searchInput.value.trim());
    }
  };
  document.getElementById('submit-btn')?.addEventListener('click', submit);
  document.getElementById('deepsearch-btn')?.addEventListener('click', () => {
    const q = searchInput.value.trim();
    if (q) enterAgent(q);
  });
  document.getElementById('websearch-btn')?.addEventListener('click', () => {
    const q = searchInput.value.trim();
    if (q) enterAgent(`search the web for ${q}`);
  });
  document.getElementById('attach-btn')?.addEventListener('click', async () => {
    const path = await Bridge.pickDirectory();
    if (path) { searchInput.value = path; searchInput.dispatchEvent(new Event('input', { bubbles: true })); }
  });

  // Auto-focus search input
  searchInput.focus();

  // When the palette is summoned via the global hotkey (Option+Space), the
  // backend emits `supersearch://reset`. Clear stale state and refocus so the
  // user always lands on an empty, ready prompt — like Spotlight.
  Bridge.listen('supersearch://reset', () => {
    // Replay the Raycast open animation each time the palette is summoned.
    const palette = document.querySelector('.palette-container');
    if (palette) {
      palette.classList.remove('rc-animate');
      void palette.offsetWidth; // force reflow so the animation restarts
      palette.classList.add('rc-animate');
      palette.style.display = 'none'; // hide it on reset until typed
    }
    searchInput.value = '';
    currentResults = [];
    selectedIndex = -1;
    if (agentMode) {
      agentMode = false;
      Agent.hide();
      agentPanel.classList.remove('active');
      contentArea.style.display = '';
      if (modeBadge) modeBadge.textContent = '⌥ Space';
    }
    Search.search('');
    renderResults(resultsPanel);
    updatePreview();
    searchInput.focus();
  });

  // Start telemetry polling
  startTelemetry();

  // Render initial empty state
  renderResults(resultsPanel);

  console.log(`[App] SuperSearch initialized (Tauri: ${Bridge.isTauri})`);
}

/**
 * Execute a search result.
 */
async function executeResult(result, withMeta, contentArea, agentPanel, modeBadge) {
  try {
    if (result._ext) {
      // Extension result — run its action through the gate-checked IPC. If it
      // has no action, there's nothing to execute (informational row).
      if (result._ext.action) {
        const execution = await Bridge.invoke('execute_extension_action', {
          id: result._ext.id,
          action: result._ext.action,
        });
        Preview.renderExecution(execution || {
          title: result.title, category: 'Extension', detail: '✓ Done', backend: `ext:${result._ext.id}`,
        });
      }
      return;
    }
    if (result.id.startsWith('agent:')) {
      // Switch to agent mode
      agentMode = true;
      contentArea.style.display = 'none';
      if (modeBadge) modeBadge.textContent = '🤖 Agent';

      const query = result.id.substring(6);
      await Agent.executeQuery(query);
    } else {
      // Regular action execution
      const execution = await Bridge.invoke('execute_action', {
        request: {
          action_id: result.id,
          with_meta: withMeta,
        }
      });
      Preview.renderExecution(execution);
    }
  } catch (error) {
    console.error('[App] Execute failed:', error);
  }
}

/**
 * Enter agent mode and run a natural-language query (used by the composer
 * submit / Deep search / Search buttons).
 * @param {string} query
 */
async function enterAgent(query) {
  agentMode = true;
  const contentArea = document.getElementById('content-area');
  const modeBadge = document.getElementById('mode-badge');
  if (contentArea) contentArea.style.display = 'none';
  if (modeBadge) modeBadge.textContent = '🤖 Agent';
  try {
    await Agent.executeQuery(query);
  } catch (err) {
    console.error('[App] Agent failed:', err);
  }
}

/**
 * Render search results into the results panel.
 * @param {HTMLElement} panel
 */
function renderResults(panel) {
  requestAnimationFrame(() => {
    if (currentResults.length === 0) {
      panel.innerHTML = `
        <div class="results-empty">
          <span class="results-empty__icon">✨</span>
          <span class="results-empty__text">Ask anything, or search apps, files &amp; commands</span>
        </div>
      `;
      return;
    }

    // Sliding selection highlight (a single element that glides between rows).
    let lastLabel = null;
    const html = ['<div class="selection-highlight" aria-hidden="true"></div>'];
    currentResults.forEach((result, i) => {
      const label = sectionLabel(result.category);
      if (label !== lastLabel) {
        lastLabel = label;
        html.push(`<div class="results-section__header">${escapeHtml(label)}</div>`);
      }
      html.push(`
        <div class="result-item ${result.category === 'Agent' ? 'result-item--agent' : ''}"
             data-index="${i}" role="option" aria-selected="false">
          <div class="result-item__icon">${result.icon}</div>
          <div class="result-item__content">
            <div class="result-item__title">${escapeHtml(result.title)}</div>
            ${result.subtitle ? `<div class="result-item__subtitle">${escapeHtml(result.subtitle)}</div>` : ''}
          </div>
          <div class="result-item__action-hint">
            <span>Search ${escapeHtml(result.title)}</span>
            <span class="result-item__action-key">tab</span>
          </div>
        </div>`);
    });
    panel.innerHTML = html.join('');

    // Click / double-click — move the highlight (no rebuild), execute on dbl.
    panel.querySelectorAll('.result-item').forEach(el => {
      el.addEventListener('click', () => {
        selectedIndex = parseInt(el.dataset.index, 10);
        positionHighlight(panel, true);
        updatePreview();
      });
      el.addEventListener('dblclick', () => {
        selectedIndex = parseInt(el.dataset.index, 10);
        const result = currentResults[selectedIndex];
        if (result) {
          executeResult(result, false,
            document.getElementById('content-area'),
            document.getElementById('agent-panel'),
            document.getElementById('mode-badge'));
        }
      });
    });

    // Place the highlight on the first row without animating (fresh list).
    positionHighlight(panel, false);
  });
}

/**
 * Move the sliding selection highlight to the active row and sync the active
 * class / aria. When `animate` is false the move is instant (used on re-render).
 * @param {HTMLElement} panel
 * @param {boolean} animate
 */
function positionHighlight(panel, animate) {
  const hl = panel.querySelector('.selection-highlight');
  panel.querySelectorAll('.result-item').forEach(r => {
    r.classList.remove('result-item--active');
    r.setAttribute('aria-selected', 'false');
  });
  if (!hl) return;
  if (selectedIndex < 0) { hl.style.opacity = '0'; return; }
  const active = panel.querySelector(`.result-item[data-index="${selectedIndex}"]`);
  if (!active) { hl.style.opacity = '0'; return; }

  active.classList.add('result-item--active');
  active.setAttribute('aria-selected', 'true');

  if (!animate) hl.classList.add('no-anim');
  hl.style.transform = `translateY(${active.offsetTop}px)`;
  hl.style.height = `${active.offsetHeight}px`;
  hl.style.opacity = '1';
  if (!animate) { void hl.offsetWidth; hl.classList.remove('no-anim'); }

  active.scrollIntoView({ block: 'nearest' });
}

/**
 * Update the preview panel with the currently selected result.
 */
function updatePreview() {
  if (!previewVisible) return;
  const result = selectedIndex >= 0 ? currentResults[selectedIndex] : null;
  Preview.render(result);
}

/**
 * Start telemetry polling — updates the telemetry strip every 500ms.
 */
function startTelemetry() {
  updateTelemetry();
  telemetryInterval = setInterval(updateTelemetry, 500);
}

/**
 * Fetch and render kernel telemetry.
 */
async function updateTelemetry() {
  try {
    const data = await Bridge.invoke('get_telemetry');
    if (!data) return;

    setTelemetryValue('telemetry-scheduler', `${data.scheduler_ticks} ticks`);
    setTelemetryValue('telemetry-capabilities', `${data.capabilities_active} active`);
    setTelemetryValue('telemetry-uptime', formatUptime(data.uptime_seconds));
    setTelemetryValue('telemetry-boot', `${data.boot_time_ms}ms`);
  } catch (err) {
    // Silently ignore telemetry failures
  }
}

function setTelemetryValue(id, value) {
  const el = document.getElementById(id);
  if (el) el.textContent = value;
}

function formatUptime(seconds) {
  if (seconds < 60) return `${Math.floor(seconds)}s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ${Math.floor(seconds % 60)}s`;
  return `${Math.floor(seconds / 3600)}h ${Math.floor((seconds % 3600) / 60)}m`;
}

function escapeHtml(str) {
  const div = document.createElement('div');
  div.textContent = str == null ? '' : String(str);
  return div.innerHTML;
}

/** Category display priority (lower = higher in the list), Raycast-style. */
const CATEGORY_RANK = { Agent: 0, Command: 1, Application: 2, Extension: 3, System: 4, Folder: 5, File: 6 };

/** Human section header for a result category. */
function sectionLabel(category) {
  switch (category) {
    case 'Agent': return 'AI Agent';
    case 'Command': return 'Commands';
    case 'Application': return 'Applications';
    case 'Extension': return 'Extensions';
    case 'System': return 'System';
    case 'File': case 'Folder': return 'Files';
    default: return category || 'Results';
  }
}

/** Order results into contiguous category groups (stable; keeps score order within a group). */
function orderResults(results) {
  return [...(results || [])].sort((a, b) => {
    const ra = CATEGORY_RANK[a.category] ?? 99;
    const rb = CATEGORY_RANK[b.category] ?? 99;
    return ra - rb; // stable sort preserves the incoming score order within a category
  });
}

// Auto-initialize
if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', init);
} else {
  init();
}
