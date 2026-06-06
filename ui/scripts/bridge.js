/**
 * SuperSearch — Tauri IPC Bridge
 *
 * Abstraction over window.__TAURI__ for WebView ↔ Kernel communication.
 * Falls back to realistic mock data when running outside Tauri (browser dev).
 */

const IS_TAURI = typeof window !== 'undefined' && window.__TAURI__ != null;

let mockUptime = 0;
let mockTicks = 0;

const MOCK_APPS = [
  { id: 'app:/Applications/Google Chrome.app', title: 'Google Chrome', subtitle: '/Applications/Google Chrome.app', category: 'Application', icon: '📱', score: 0.9 },
  { id: 'app:/Applications/Visual Studio Code.app', title: 'Visual Studio Code', subtitle: '/Applications/Visual Studio Code.app', category: 'Application', icon: '📱', score: 0.85 },
  { id: 'app:/Applications/Slack.app', title: 'Slack', subtitle: '/Applications/Slack.app', category: 'Application', icon: '📱', score: 0.8 },
  { id: 'app:/Applications/Spotify.app', title: 'Spotify', subtitle: '/Applications/Spotify.app', category: 'Application', icon: '📱', score: 0.75 },
  { id: 'app:/System/Applications/Terminal.app', title: 'Terminal', subtitle: '/System/Applications/Terminal.app', category: 'Application', icon: '📱', score: 0.7 },
  { id: 'app:/System/Applications/Calculator.app', title: 'Calculator', subtitle: '/System/Applications/Calculator.app', category: 'Application', icon: '📱', score: 0.65 },
  { id: 'app:/Applications/Figma.app', title: 'Figma', subtitle: '/Applications/Figma.app', category: 'Application', icon: '📱', score: 0.6 },
  { id: 'app:/Applications/Discord.app', title: 'Discord', subtitle: '/Applications/Discord.app', category: 'Application', icon: '📱', score: 0.55 },
];

const MOCK_SYS_COMMANDS = [
  { id: 'sys:lock', title: 'Lock Screen', subtitle: 'Lock the screen immediately', category: 'System', icon: '🔒', score: 0.0 },
  { id: 'sys:screenshot', title: 'Screenshot', subtitle: 'Capture a screenshot', category: 'System', icon: '📸', score: 0.0 },
  { id: 'sys:dnd', title: 'Do Not Disturb', subtitle: 'Toggle Do Not Disturb mode', category: 'System', icon: '🔕', score: 0.0 },
  { id: 'sys:dark_mode', title: 'Toggle Dark Mode', subtitle: 'Switch between light and dark appearance', category: 'System', icon: '🌗', score: 0.0 },
  { id: 'sys:clipboard', title: 'Show Clipboard', subtitle: 'View current clipboard contents', category: 'System', icon: '📋', score: 0.0 },
  { id: 'sys:running_apps', title: 'Running Apps', subtitle: 'List all currently running applications', category: 'System', icon: '📊', score: 0.0 },
  { id: 'sys:system_info', title: 'System Info', subtitle: 'View system information', category: 'System', icon: 'ℹ️', score: 0.0 },
];

// In-memory extension list for browser-dev mock mode.
let MOCK_EXTENSIONS = [
  {
    id: 'ddg', name: 'DuckDuckGo Search', version: '1.0.0', author: 'SuperSearch',
    description: 'Open a DuckDuckGo web search for your query', kind: 'script', enabled: false,
    permissions: [{ permission: 'NetworkConnect', justification: 'Open search results in your default browser' }],
  },
];

// Simple agent intent detection for mock mode
const AGENT_PATTERNS = [
  /^open\s+/i, /^launch\s+/i, /^start\s+/i, /^quit\s+/i, /^close\s+/i,
  /^find\s+/i, /^search\s+/i, /^switch to\s+/i, /^focus\s+/i,
  /^lock/i, /^screenshot/i, /^clipboard/i, /^running/i, /^what's running/i,
  /^volume/i, /^mute/i, /^brightness/i, /^dark mode/i, /^dnd/i,
  /^sleep/i, /^empty trash/i, /^battery/i, /^disk/i, /^uptime/i,
  /^https?:\/\//i, /^www\./i,
];

export const Bridge = {
  isTauri: IS_TAURI,

  async invoke(cmd, args = {}) {
    if (IS_TAURI) {
      return window.__TAURI__.core.invoke(cmd, args);
    }

    // Mock fallback for browser development
    switch (cmd) {
      case 'search_query': {
        const q = (args.query || '').toLowerCase();
        if (!q) return [];

        let results = [];

        // App search
        const appResults = MOCK_APPS.filter(a =>
          a.title.toLowerCase().includes(q)
        ).map(a => ({ ...a, score: a.title.toLowerCase().startsWith(q) ? 0.95 : 0.7 }));
        results.push(...appResults);

        // System commands
        const sysResults = MOCK_SYS_COMMANDS.filter(c =>
          c.title.toLowerCase().includes(q) || c.subtitle.toLowerCase().includes(q)
        ).map(c => ({ ...c, score: 0.6 }));
        results.push(...sysResults);

        // Agent detection
        if (AGENT_PATTERNS.some(p => p.test(args.query || ''))) {
          results.unshift({
            id: `agent:${args.query}`,
            title: `⚡ ${args.query}`,
            subtitle: 'Execute with AI Agent',
            category: 'Agent',
            icon: '🤖',
            score: 1.5,
          });
        }

        results.sort((a, b) => b.score - a.score);
        return results.slice(0, 12);
      }

      case 'execute_action': {
        const actionId = args.action_id || '';
        if (actionId.startsWith('agent:')) {
          const query = actionId.substring(6);
          return {
            action_id: actionId,
            acknowledged: true,
            title: 'Agent Execution',
            category: 'Agent',
            detail: `Mock agent executed: "${query}"`,
            backend: 'mock-agent',
          };
        }
        if (actionId.startsWith('app:')) {
          return {
            action_id: actionId,
            acknowledged: true,
            title: 'Launch App',
            category: 'Application',
            detail: `✓ Launched ${actionId.split('/').pop().replace('.app', '')}`,
            backend: 'mock-os',
          };
        }
        return {
          action_id: actionId,
          acknowledged: true,
          title: 'Action Executed',
          category: 'System',
          detail: `Mock executed: ${actionId}`,
          backend: 'mock-backend',
        };
      }

      case 'agent_query': {
        const q = args.query || '';
        await new Promise(r => setTimeout(r, 300)); // Simulate processing
        return {
          query: q,
          intent: `Mock Intent: ${q}`,
          plan_description: q,
          total_steps: 1,
          steps: [{
            label: q,
            success: true,
            output: `✓ Mock executed: ${q}`,
            error: null,
          }],
          success: true,
          summary: `✓ ${q} completed (mock)`,
          duration_ms: 42,
        };
      }

      case 'agent_check': {
        const q = (args.query || '').trim();
        return AGENT_PATTERNS.some(p => p.test(q));
      }

      case 'hide_window':
        return true;

      case 'list_extensions':
        return MOCK_EXTENSIONS.map((e) => ({ ...e }));

      case 'install_extension': {
        const id = (args.path || '').split('/').filter(Boolean).pop() || `ext-${Date.now()}`;
        if (!MOCK_EXTENSIONS.some((e) => e.id === id)) {
          MOCK_EXTENSIONS.push({
            id, name: id, version: '0.0.0', author: null, description: 'Installed (mock)',
            kind: 'script', enabled: false, permissions: [],
          });
        }
        return id;
      }

      case 'uninstall_extension':
        MOCK_EXTENSIONS = MOCK_EXTENSIONS.filter((e) => e.id !== args.id);
        return null;

      case 'set_extension_enabled': {
        const ext = MOCK_EXTENSIONS.find((e) => e.id === args.id);
        if (ext) ext.enabled = !!args.enabled;
        return null;
      }

      case 'query_extensions':
        return [];

      case 'execute_extension_action':
        return null;

      case 'get_telemetry':
        mockUptime += 0.5;
        mockTicks += Math.floor(Math.random() * 3);
        return {
          scheduler_ticks: mockTicks,
          scheduler_idle: true,
          capabilities_active: 3 + Math.floor(Math.random() * 3),
          capabilities_total: 5,
          uptime_seconds: mockUptime,
          boot_time_ms: 12,
        };

      default:
        console.warn(`[Bridge] Unknown command: ${cmd}`);
        return null;
    }
  },

  async listen(event, callback) {
    if (IS_TAURI) {
      return window.__TAURI__.event.listen(event, callback);
    }
    return () => {};
  },

  /**
   * Open a native folder picker, returning the selected path or null.
   * Falls back to a prompt in browser-dev or if the dialog API is absent.
   * @returns {Promise<string|null>}
   */
  async pickDirectory() {
    if (IS_TAURI && window.__TAURI__.dialog?.open) {
      const selected = await window.__TAURI__.dialog.open({ directory: true, multiple: false });
      return typeof selected === 'string' ? selected : null;
    }
    const path = typeof prompt === 'function' ? prompt('Extension folder path:') : null;
    return path && path.trim() ? path.trim() : null;
  },
};
