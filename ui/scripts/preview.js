/**
 * SuperSearch — Preview Panel
 *
 * Renders contextual preview for the selected search result.
 */

/** @type {HTMLElement|null} */
let previewElement = null;

/**
 * Initialize the preview module.
 * @param {HTMLElement} el - The preview panel DOM element
 */
export function init(el) {
  previewElement = el;
  renderEmpty();
}

/**
 * Render preview for a search result.
 * @param {SearchResult|null} result
 */
export function render(result) {
  if (!previewElement) return;

  if (!result) {
    renderEmpty();
    return;
  }

  const meta = getResultMeta(result);

  previewElement.innerHTML = `
    <div class="preview-header">
      <span class="preview-header__icon">${result.icon}</span>
      <span class="preview-header__title">${escapeHtml(result.title)}</span>
    </div>
    <div class="preview-body">
      <p class="preview-body__description">${escapeHtml(result.subtitle)}</p>
    </div>
    <div class="preview-meta">
      ${meta.map(([label, value]) => `
        <div class="preview-meta__row">
          <span class="preview-meta__label">${escapeHtml(label)}</span>
          <span class="preview-meta__value">${escapeHtml(value)}</span>
        </div>
      `).join('')}
    </div>
  `;

  // Re-trigger animation
  previewElement.style.animation = 'none';
  previewElement.offsetHeight; // force reflow
  previewElement.style.animation = '';
}

/**
 * Render a backend execution response in the preview panel.
 * @param {{action_id: string, acknowledged: boolean, title: string, category: string, detail: string, backend: string}} execution
 */
export function renderExecution(execution) {
  if (!previewElement) return;

  previewElement.innerHTML = `
    <div class="preview-header">
      <span class="preview-header__icon">↵</span>
      <span class="preview-header__title">${escapeHtml(execution.title || 'Action executed')}</span>
    </div>
    <div class="preview-body">
      <p class="preview-body__description">${escapeHtml(execution.detail || 'The backend handled the request.')}</p>
    </div>
    <div class="preview-meta">
      <div class="preview-meta__row">
        <span class="preview-meta__label">Action</span>
        <span class="preview-meta__value">${escapeHtml(execution.action_id)}</span>
      </div>
      <div class="preview-meta__row">
        <span class="preview-meta__label">Category</span>
        <span class="preview-meta__value">${escapeHtml(execution.category || 'Action')}</span>
      </div>
      <div class="preview-meta__row">
        <span class="preview-meta__label">Backend</span>
        <span class="preview-meta__value">${escapeHtml(execution.backend || 'rust-runtime')}</span>
      </div>
      <div class="preview-meta__row">
        <span class="preview-meta__label">Status</span>
        <span class="preview-meta__value">${execution.acknowledged ? 'Acknowledged' : 'Pending'}</span>
      </div>
    </div>
  `;

  previewElement.style.animation = 'none';
  previewElement.offsetHeight;
  previewElement.style.animation = '';
}

/**
 * Render the empty state for the preview panel.
 */
function renderEmpty() {
  if (!previewElement) return;
  previewElement.innerHTML = `
    <div class="preview-panel__empty">
      <span class="preview-panel__empty-icon">◇</span>
      <span class="preview-panel__empty-text">Select a result to preview details</span>
    </div>
  `;
}

/**
 * Extract metadata key-value pairs from a result.
 * @param {SearchResult} result
 * @returns {[string, string][]}
 */
function getResultMeta(result) {
  const meta = [
    ['Category', result.category],
    ['ID', result.id],
  ];

  if (result.id.startsWith('cmd:')) {
    meta.push(['Type', 'Kernel Command']);
    meta.push(['Priority', 'UserBlocking']);
  } else if (result.id.startsWith('act:')) {
    meta.push(['Type', 'Action']);
    meta.push(['Priority', 'Default']);
  }

  meta.push(['Relevance', `${Math.round(result.score * 100)}%`]);
  return meta;
}

/**
 * Escape HTML to prevent XSS.
 * @param {string} str
 * @returns {string}
 */
function escapeHtml(str) {
  const div = document.createElement('div');
  div.textContent = str;
  return div.innerHTML;
}
