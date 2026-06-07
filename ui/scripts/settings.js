/**
 * SuperSearch — Settings panel
 *
 * Reads/writes persisted settings via IPC (get_settings/update_settings):
 * the global hotkey, hide-on-blur, and theme. Applies the theme immediately.
 */

import { Bridge } from './bridge.js';

let overlay, hotkeyInput, blurInput, themeSelect, statusEl;
let initialized = false;
let current = null;

export function init() {
  overlay = document.getElementById('settings-overlay');
  hotkeyInput = document.getElementById('settings-hotkey');
  blurInput = document.getElementById('settings-blur');
  themeSelect = document.getElementById('settings-theme');
  statusEl = document.getElementById('settings-status');
  if (!overlay) return;

  document.getElementById('settings-btn')?.addEventListener('click', open);
  document.getElementById('settings-close-btn')?.addEventListener('click', close);
  document.getElementById('settings-save-btn')?.addEventListener('click', save);
  overlay.addEventListener('mousedown', (e) => { if (e.target === overlay) close(); });

  // Apply the persisted theme on startup (before the panel is ever opened).
  loadAndApplyTheme();
  initialized = true;
}

export function isOpen() {
  return initialized && overlay && !overlay.hidden;
}

export async function open() {
  if (!initialized) return;
  await refresh();
  overlay.hidden = false;
  if (statusEl) statusEl.textContent = '';
  hotkeyInput?.focus();
}

export function close() {
  if (initialized) overlay.hidden = true;
}

async function refresh() {
  try {
    current = (await Bridge.invoke('get_settings')) || {};
  } catch (err) {
    console.error('[Settings] load failed:', err);
    current = {};
  }
  if (hotkeyInput) hotkeyInput.value = current.toggle_shortcut || 'Alt+Space';
  if (blurInput) blurInput.checked = current.hide_on_blur !== false;
  if (themeSelect) themeSelect.value = current.theme || 'dark';
}

async function save() {
  const next = {
    toggle_shortcut: (hotkeyInput?.value || 'Alt+Space').trim() || 'Alt+Space',
    hide_on_blur: !!blurInput?.checked,
    theme: themeSelect?.value || 'dark',
  };
  try {
    await Bridge.invoke('update_settings', { settings: next });
    current = next;
    applyTheme(next.theme);
    if (statusEl) statusEl.textContent = 'Saved ✓';
    setTimeout(() => { if (statusEl) statusEl.textContent = ''; }, 1500);
  } catch (err) {
    console.error('[Settings] save failed:', err);
    if (statusEl) statusEl.textContent = `Error: ${err}`;
  }
}

async function loadAndApplyTheme() {
  try {
    const s = (await Bridge.invoke('get_settings')) || {};
    applyTheme(s.theme || 'dark');
  } catch (_) { /* ignore */ }
}

function applyTheme(theme) {
  document.body.dataset.theme = theme;
}
