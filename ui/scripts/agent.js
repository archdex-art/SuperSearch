/**
 * SuperSearch — Agent Console Module
 *
 * Handles AI agent interactions:
 * - Sends natural language queries to the agent backend
 * - Renders workflow plans and execution steps
 * - Streams runtime events for real-time updates
 */

import { Bridge } from './bridge.js';

/** @type {HTMLElement|null} */
let agentPanel = null;

/** @type {boolean} */
let isProcessing = false;

/**
 * Initialize the agent module.
 * @param {HTMLElement} panel - The agent panel DOM element
 */
export function init(panel) {
  agentPanel = panel;
}

/**
 * Check if a query should be handled by the agent.
 * @param {string} query
 * @returns {Promise<boolean>}
 */
export async function isAgentQuery(query) {
  if (!query || query.trim().length < 2) return false;
  try {
    return await Bridge.invoke('agent_check', { query: query.trim() });
  } catch {
    return false;
  }
}

/**
 * Execute an agent query and render results.
 * @param {string} query
 * @returns {Promise<object|null>}
 */
export async function executeQuery(query) {
  if (isProcessing) return null;
  isProcessing = true;

  renderProcessing(query);

  try {
    const response = await Bridge.invoke('agent_query', { query: query.trim() });
    renderResponse(response);
    isProcessing = false;
    return response;
  } catch (err) {
    renderError(query, err);
    isProcessing = false;
    return null;
  }
}

/**
 * Render a processing/thinking state.
 * @param {string} query
 */
function renderProcessing(query) {
  if (!agentPanel) return;
  agentPanel.innerHTML = `
    <div class="agent-response agent-response--processing">
      <div class="agent-header">
        <span class="agent-header__icon">🤖</span>
        <span class="agent-header__title">Processing…</span>
      </div>
      <div class="agent-body">
        <div class="agent-query">${escapeHtml(query)}</div>
        <div class="agent-thinking">
          <span class="typing-dot"></span>
          <span class="typing-dot"></span>
          <span class="typing-dot"></span>
        </div>
      </div>
    </div>
  `;
  agentPanel.classList.add('active');
}

/**
 * Render a completed agent response.
 * @param {object} response
 */
function renderResponse(response) {
  if (!agentPanel) return;

  const stepsHtml = response.steps.map((step, i) => `
    <div class="agent-step ${step.success ? 'agent-step--success' : 'agent-step--error'}">
      <div class="agent-step__indicator">
        ${step.success ? '✓' : '✗'}
      </div>
      <div class="agent-step__content">
        <div class="agent-step__label">${escapeHtml(step.label)}</div>
        ${step.output ? `<div class="agent-step__output">${escapeHtml(truncate(step.output, 200))}</div>` : ''}
        ${step.error ? `<div class="agent-step__error">${escapeHtml(step.error)}</div>` : ''}
      </div>
    </div>
  `).join('');

  agentPanel.innerHTML = `
    <div class="agent-response ${response.success ? 'agent-response--success' : 'agent-response--error'}">
      <div class="agent-header">
        <span class="agent-header__icon">${response.success ? '✓' : '⚠'}</span>
        <span class="agent-header__title">${escapeHtml(response.intent)}</span>
        <span class="agent-header__timing">${response.duration_ms}ms</span>
      </div>
      <div class="agent-body">
        <div class="agent-summary">${escapeHtml(response.summary)}</div>
        ${response.steps.length > 0 ? `
          <div class="agent-steps">
            <div class="agent-steps__header">
              Workflow — ${response.plan_description}
              <span class="agent-steps__count">${response.total_steps} step${response.total_steps !== 1 ? 's' : ''}</span>
            </div>
            ${stepsHtml}
          </div>
        ` : ''}
      </div>
    </div>
  `;
  agentPanel.classList.add('active');

  // Trigger entrance animation.
  agentPanel.style.animation = 'none';
  agentPanel.offsetHeight;
  agentPanel.style.animation = '';
}

/**
 * Render an error state.
 * @param {string} query
 * @param {*} err
 */
function renderError(query, err) {
  if (!agentPanel) return;
  agentPanel.innerHTML = `
    <div class="agent-response agent-response--error">
      <div class="agent-header">
        <span class="agent-header__icon">✗</span>
        <span class="agent-header__title">Agent Error</span>
      </div>
      <div class="agent-body">
        <div class="agent-query">${escapeHtml(query)}</div>
        <div class="agent-step__error">${escapeHtml(String(err))}</div>
      </div>
    </div>
  `;
  agentPanel.classList.add('active');
}

/**
 * Hide the agent panel.
 */
export function hide() {
  if (agentPanel) {
    agentPanel.classList.remove('active');
    agentPanel.innerHTML = '';
  }
  isProcessing = false;
}

/**
 * Whether the agent is currently processing.
 */
export function busy() {
  return isProcessing;
}

/** Escape HTML */
function escapeHtml(str) {
  const div = document.createElement('div');
  div.textContent = str;
  return div.innerHTML;
}

/** Truncate string */
function truncate(str, max) {
  if (str.length <= max) return str;
  return str.substring(0, max) + '…';
}
