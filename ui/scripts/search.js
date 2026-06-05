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
      const results = await Bridge.invoke('search_query', { query: trimmed });
      cachedResults = results || [];
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
