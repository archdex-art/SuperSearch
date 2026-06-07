/**
 * SuperSearch — Extension Manager
 *
 * Drives the Plugin Manager overlay: list installed extensions, install from a
 * folder, enable (with a permission-consent step), disable, and uninstall.
 * All actions go through the Tauri IPC commands backed by ExtensionRegistry.
 */

import { Bridge } from './bridge.js';

let overlay, listEl, consentEl, consentTitle, consentPerms, consentCancel, consentConfirm;
let initialized = false;

/** Resolver for the in-flight consent prompt, if any. */
let consentResolve = null;

/**
 * Wire up DOM references and event handlers. Call once on startup.
 */
export function init() {
  overlay = document.getElementById('extensions-overlay');
  listEl = document.getElementById('ext-list');
  consentEl = document.getElementById('ext-consent');
  consentTitle = document.getElementById('ext-consent-title');
  consentPerms = document.getElementById('ext-consent-perms');
  consentCancel = document.getElementById('ext-consent-cancel');
  consentConfirm = document.getElementById('ext-consent-confirm');

  if (!overlay) return;

  document.getElementById('extensions-btn')?.addEventListener('click', open);
  document.getElementById('ext-close-btn')?.addEventListener('click', close);
  document.getElementById('ext-install-btn')?.addEventListener('click', installFlow);

  // Click the dimmed backdrop (but not the panel) to dismiss.
  overlay.addEventListener('mousedown', (e) => {
    if (e.target === overlay) close();
  });

  consentCancel?.addEventListener('click', () => resolveConsent(false));
  consentConfirm?.addEventListener('click', () => resolveConsent(true));

  initialized = true;
}

/** @returns {boolean} whether the manager overlay is currently open. */
export function isOpen() {
  return initialized && overlay && !overlay.hidden;
}

/** Open the manager and refresh the list. */
export async function open() {
  if (!initialized) return;
  overlay.hidden = false;
  await refresh();
}

/** Close the manager (and any open consent prompt). */
export function close() {
  if (!initialized) return;
  resolveConsent(false);
  overlay.hidden = true;
}

/** Fetch the installed extensions and render the list. */
async function refresh() {
  let extensions = [];
  try {
    extensions = (await Bridge.invoke('list_extensions')) || [];
  } catch (err) {
    console.error('[Extensions] list failed:', err);
  }
  render(extensions);
}

function render(extensions) {
  if (extensions.length === 0) {
    listEl.innerHTML = `
      <div class="ext-empty">
        <span class="ext-empty__icon">🧩</span>
        <span class="ext-empty__text">No extensions installed</span>
        <span class="ext-empty__hint">Click “Install…” to add one from a folder.</span>
      </div>`;
    return;
  }

  listEl.innerHTML = extensions.map((ext) => {
    const perms = (ext.permissions || [])
      .map((p) => `<span class="ext-perm" title="${escapeHtml(p.justification)}">${escapeHtml(p.permission)}</span>`)
      .join('');
    const author = ext.author ? ` · ${escapeHtml(ext.author)}` : '';
    return `
      <div class="ext-card" role="listitem" data-id="${escapeHtml(ext.id)}">
        <div class="ext-card__main">
          <div class="ext-card__head">
            <span class="ext-card__name">${escapeHtml(ext.name)}</span>
            <span class="ext-card__meta">v${escapeHtml(ext.version)}${author} · ${escapeHtml(ext.kind)}</span>
          </div>
          ${ext.description ? `<div class="ext-card__desc">${escapeHtml(ext.description)}</div>` : ''}
          ${perms ? `<div class="ext-card__perms">${perms}</div>` : ''}
        </div>
        <div class="ext-card__controls">
          <label class="ext-toggle" title="${ext.enabled ? 'Enabled' : 'Disabled'}">
            <input type="checkbox" class="ext-toggle__input" data-action="toggle" ${ext.enabled ? 'checked' : ''}>
            <span class="ext-toggle__track"><span class="ext-toggle__thumb"></span></span>
          </label>
          <button class="ext-btn ext-btn--danger ext-btn--sm" data-action="uninstall" type="button">Uninstall</button>
        </div>
      </div>`;
  }).join('');

  listEl.querySelectorAll('.ext-card').forEach((card) => {
    const id = card.dataset.id;
    const ext = extensions.find((e) => e.id === id);
    card.querySelector('[data-action="toggle"]')?.addEventListener('change', (e) => {
      toggleExtension(ext, e.target.checked);
    });
    card.querySelector('[data-action="uninstall"]')?.addEventListener('click', () => {
      uninstallExtension(ext);
    });
  });
}

/** Enable (with consent) or disable an extension. */
async function toggleExtension(ext, enabled) {
  try {
    if (enabled && (ext.permissions || []).length > 0) {
      const granted = await requestConsent(ext);
      if (!granted) {
        await refresh(); // revert the toggle UI
        return;
      }
    }
    await Bridge.invoke('set_extension_enabled', { id: ext.id, enabled });
  } catch (err) {
    console.error('[Extensions] toggle failed:', err);
  }
  await refresh();
}

async function uninstallExtension(ext) {
  if (!confirm(`Uninstall “${ext.name}”? This removes its files.`)) return;
  try {
    await Bridge.invoke('uninstall_extension', { id: ext.id });
  } catch (err) {
    console.error('[Extensions] uninstall failed:', err);
    alert(`Uninstall failed: ${err}`);
  }
  await refresh();
}

/** Pick a folder and install the extension it contains. */
async function installFlow() {
  let dir;
  try {
    dir = await Bridge.pickDirectory();
  } catch (err) {
    console.error('[Extensions] folder pick failed:', err);
    return;
  }
  if (!dir) return;
  try {
    await Bridge.invoke('install_extension', { path: dir });
  } catch (err) {
    console.error('[Extensions] install failed:', err);
    alert(`Install failed: ${err}`);
  }
  await refresh();
}

/**
 * Show the permission-consent modal and resolve to true if the user allows.
 * @returns {Promise<boolean>}
 */
function requestConsent(ext) {
  consentTitle.textContent = `Enable “${ext.name}”`;
  consentPerms.innerHTML = (ext.permissions || []).map((p) => `
    <li class="ext-consent__perm">
      <span class="ext-consent__perm-name">${escapeHtml(p.permission)}</span>
      <span class="ext-consent__perm-why">${escapeHtml(p.justification)}</span>
    </li>`).join('');
  consentEl.hidden = false;
  return new Promise((resolve) => { consentResolve = resolve; });
}

function resolveConsent(value) {
  if (consentResolve) {
    consentResolve(value);
    consentResolve = null;
  }
  if (consentEl) consentEl.hidden = true;
}

function escapeHtml(str) {
  const div = document.createElement('div');
  div.textContent = str == null ? '' : String(str);
  return div.innerHTML;
}
