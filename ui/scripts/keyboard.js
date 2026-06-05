/**
 * SuperSearch — Keyboard Navigation
 *
 * Manages keyboard shortcuts for the command palette.
 * All bindings use `keydown` with `event.key`.
 */

/** @type {function|null} */
let onNavigateCallback = null;
/** @type {function|null} */
let onExecuteCallback = null;
/** @type {function|null} */
let onDismissCallback = null;
/** @type {function|null} */
let onTogglePreviewCallback = null;

/**
 * Register navigation callbacks.
 */
export function registerCallbacks({ onNavigate, onExecute, onDismiss, onTogglePreview }) {
  onNavigateCallback = onNavigate || null;
  onExecuteCallback = onExecute || null;
  onDismissCallback = onDismiss || null;
  onTogglePreviewCallback = onTogglePreview || null;
}

/**
 * Initialize keyboard event listeners.
 * Call once on app startup.
 */
export function init() {
  document.addEventListener('keydown', handleKeyDown);
}

/**
 * Teardown keyboard listeners.
 */
export function destroy() {
  document.removeEventListener('keydown', handleKeyDown);
}

/**
 * Handle keydown events.
 * @param {KeyboardEvent} e
 */
function handleKeyDown(e) {
  switch (e.key) {
    case 'ArrowDown':
      e.preventDefault();
      if (onNavigateCallback) onNavigateCallback(1);
      break;

    case 'ArrowUp':
      e.preventDefault();
      if (onNavigateCallback) onNavigateCallback(-1);
      break;

    case 'Enter':
      e.preventDefault();
      if (onExecuteCallback) onExecuteCallback(e.metaKey);
      break;

    case 'Escape':
      e.preventDefault();
      if (onDismissCallback) onDismissCallback();
      break;

    case 'Tab':
      e.preventDefault();
      if (onTogglePreviewCallback) onTogglePreviewCallback();
      break;

    default:
      break;
  }
}
