/**
 * SuperSearch — Search Engine Client
 *
 * Debounced search with result caching and IPC integration.
 */

import { Bridge } from './bridge.js';

/** @type {number|null} */
let debounceTimer = null;

/** @type {SearchResult[]} */
let cachedResults = [];

/** @type {string} */
let lastQuery = '';

/** @type {function|null} */
let onResultsCallback = null;

/**
 * Register a callback for search results.
 * @param {function(SearchResult[]): void} callback
 */
export function onResults(callback) {
  onResultsCallback = callback;
}

/**
 * Perform a debounced search.
 * Skips queries shorter than 1 character and debounces at 50ms.
 * @param {string} query - Search query string
 */
export function search(query) {
  const trimmed = query.trim();

  // Clear results for empty queries immediately
  if (!trimmed) {
    clearTimeout(debounceTimer);
    lastQuery = '';
    cachedResults = [];
    if (onResultsCallback) onResultsCallback([]);
    return;
  }

  // Skip if query hasn't changed
  if (trimmed === lastQuery) return;

  // Debounce
  clearTimeout(debounceTimer);
  debounceTimer = setTimeout(async () => {
    lastQuery = trimmed;
    try {
      // Native results and enabled-extension results are fetched in parallel
      // and merged. Extension hits carry an `_ext` payload (source id + action)
      // so execution can route through execute_extension_action (gate-checked).
      const [native, extHits] = await Promise.all([
        Bridge.invoke('search_query', { query: trimmed }),
        Bridge.invoke('query_extensions', { query: trimmed }).catch(() => []),
      ]);

      const extResults = (extHits || []).map((h, i) => ({
        id: `ext:${h.extension_id}:${i}`,
        title: h.title,
        subtitle: h.subtitle || '',
        category: 'Extension',
        icon: '🧩',
        score: 1.2,
        _ext: { id: h.extension_id, action: h.action || null },
      }));

      const merged = [...(native || []), ...extResults];
      merged.sort((a, b) => (b.score || 0) - (a.score || 0));
      cachedResults = merged;
      if (onResultsCallback) onResultsCallback(cachedResults);
    } catch (err) {
      console.error('[Search] Query failed:', err);
      cachedResults = [];
      if (onResultsCallback) onResultsCallback([]);
    }
  }, 50);
}

/**
 * Get the currently cached results.
 * @returns {SearchResult[]}
 */
export function getResults() {
  return cachedResults;
}

/**
 * Clear the search state.
 */
export function clear() {
  clearTimeout(debounceTimer);
  lastQuery = '';
  cachedResults = [];
  if (onResultsCallback) onResultsCallback([]);
}
