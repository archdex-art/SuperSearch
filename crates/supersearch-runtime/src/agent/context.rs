//! System context engine — tracks active apps, recent files, workspace state.
//!
//! Provides runtime context signals for search ranking, intent disambiguation,
//! and proactive suggestions. Updated periodically by the kernel.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

const CONTEXT_TTL: Duration = Duration::from_secs(3600); // 1 hour TTL for stale context
const MAX_CONTEXT_PROVIDERS: usize = 10;
const MAX_PAYLOAD_BYTES: usize = 8192; // Max 8KB per context provider to prevent token exhaustion

/// A rolling window of semantic context provided by extensions.
/// Designed to prevent token overload when sending state to the LLM.
#[derive(Debug, Clone)]
pub struct ContextWindow {
    /// Foreground context (highest priority). E.g., The active IDE file.
    active_providers: HashMap<String, ContextItem>,
    /// Background context (evicted first when token limits are reached).
    background_providers: HashMap<String, ContextItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextItem {
    pub extension_id: String,
    pub payload: String, // Stringified JSON or Markdown semantic state
    #[serde(skip, default = "Instant::now")]
    pub timestamp: Instant,
}

impl ContextWindow {
    pub fn new() -> Self {
        Self {
            active_providers: HashMap::new(),
            background_providers: HashMap::new(),
        }
    }
    /// Registers or updates context from an extension, enforcing hard budgeting limits.
    pub fn push_context(&mut self, extension_id: &str, mut payload: String, is_active: bool) {
        // Enforce maximum payload size (Context Budgeting M5 Feedback)
        if payload.len() > MAX_PAYLOAD_BYTES {
            payload.truncate(MAX_PAYLOAD_BYTES);
            payload.push_str("...[TRUNCATED]");
        }

        let item = ContextItem {
            extension_id: extension_id.to_string(),
            payload,
            timestamp: Instant::now(),
        };

        if is_active {
            self.active_providers.insert(extension_id.to_string(), item);
        } else {
            self.background_providers
                .insert(extension_id.to_string(), item);
        }

        // Enforce maximum active providers (Context Budgeting M5 Feedback)
        if self.active_providers.len() > MAX_CONTEXT_PROVIDERS {
            // Evict the oldest context
            if let Some(oldest_key) = self
                .active_providers
                .iter()
                .min_by_key(|entry| entry.1.timestamp)
                .map(|(k, _)| k.clone())
            {
                self.active_providers.remove(&oldest_key);
            }
        }
    }

    /// Evicts stale context that has surpassed the TTL.
    pub fn prune_stale_context(&mut self) {
        let now = Instant::now();
        self.active_providers
            .retain(|_, item| now.duration_since(item.timestamp) < CONTEXT_TTL);
        self.background_providers
            .retain(|_, item| now.duration_since(item.timestamp) < CONTEXT_TTL);
    }

    /// Flattens the prioritized context into a single prompt-ready string for the LLM.
    /// In production, this includes token counting to strictly cap at e.g. 16k tokens.
    pub fn flatten_for_llm(&self) -> String {
        let mut out = String::from("<context>\n");
        for (id, item) in &self.active_providers {
            out.push_str(&format!(
                "<provider id=\"{}\" state=\"active\">\n{}\n</provider>\n",
                id, item.payload
            ));
        }
        for (id, item) in &self.background_providers {
            out.push_str(&format!(
                "<provider id=\"{}\" state=\"background\">\n{}\n</provider>\n",
                id, item.payload
            ));
        }
        out.push_str("</context>");
        out
    }
}

/// Maximum recent items to track per category.
const MAX_RECENT: usize = 20;

/// System context snapshot used for ranking and suggestions.
pub struct ContextEngine {
    /// Currently active (frontmost) application.
    pub active_app: Option<String>,
    /// Recently used applications (most recent first).
    pub recent_apps: VecDeque<String>,
    /// Recently opened files (most recent first).
    pub recent_files: VecDeque<String>,
    /// Inferred workspace context.
    pub workspace: WorkspaceContext,
    /// Last context update time.
    pub last_update: Instant,
    /// Rolling window of semantic context provided by extensions.
    pub extension_context: ContextWindow,
}

/// Inferred workspace context based on active applications.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WorkspaceContext {
    /// Development (VS Code, Terminal, Xcode, etc.)
    Development,
    /// Communication (Slack, Teams, Discord, Mail)
    Communication,
    /// Creative (Figma, Photoshop, Final Cut)
    Creative,
    /// Productivity (Office, Notes, Calendar)
    Productivity,
    /// Browsing (Safari, Chrome)
    Browsing,
    /// Unknown / mixed
    General,
}

impl Default for ContextEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextEngine {
    pub fn new() -> Self {
        Self {
            active_app: None,
            recent_apps: VecDeque::with_capacity(MAX_RECENT),
            recent_files: VecDeque::with_capacity(MAX_RECENT),
            workspace: WorkspaceContext::General,
            last_update: Instant::now(),
            extension_context: ContextWindow::new(),
        }
    }

    /// Record an application being used.
    pub fn record_app(&mut self, app_name: &str) {
        self.active_app = Some(app_name.to_string());

        // Move to front of recent list.
        self.recent_apps.retain(|a| a != app_name);
        self.recent_apps.push_front(app_name.to_string());
        if self.recent_apps.len() > MAX_RECENT {
            self.recent_apps.pop_back();
        }

        self.infer_workspace();
        self.last_update = Instant::now();
    }

    /// Record a file being opened.
    pub fn record_file(&mut self, path: &str) {
        self.recent_files.retain(|f| f != path);
        self.recent_files.push_front(path.to_string());
        if self.recent_files.len() > MAX_RECENT {
            self.recent_files.pop_back();
        }
        self.last_update = Instant::now();
    }

    /// Compute a relevance boost for an app based on recent usage.
    pub fn app_relevance(&self, app_name: &str) -> f64 {
        let lower = app_name.to_lowercase();
        for (i, app) in self.recent_apps.iter().enumerate() {
            if app.to_lowercase() == lower {
                return 1.0 - (i as f64 / MAX_RECENT as f64) * 0.5;
            }
        }
        0.0
    }

    /// Infer workspace context from recent apps.
    fn infer_workspace(&mut self) {
        let dev_apps = [
            "code", "terminal", "iterm", "xcode", "intellij", "neovim", "warp", "cursor",
        ];
        let comm_apps = ["slack", "teams", "discord", "mail", "zoom", "messages"];
        let creative_apps = [
            "figma",
            "photoshop",
            "illustrator",
            "final cut",
            "premiere",
            "sketch",
        ];
        let prod_apps = [
            "pages", "numbers", "keynote", "word", "excel", "notes", "notion",
        ];

        let recent: Vec<String> = self
            .recent_apps
            .iter()
            .take(5)
            .map(|a| a.to_lowercase())
            .collect();

        let dev_count = recent
            .iter()
            .filter(|a| dev_apps.iter().any(|d| a.contains(d)))
            .count();
        let comm_count = recent
            .iter()
            .filter(|a| comm_apps.iter().any(|d| a.contains(d)))
            .count();
        let creative_count = recent
            .iter()
            .filter(|a| creative_apps.iter().any(|d| a.contains(d)))
            .count();
        let prod_count = recent
            .iter()
            .filter(|a| prod_apps.iter().any(|d| a.contains(d)))
            .count();

        let max = dev_count
            .max(comm_count)
            .max(creative_count)
            .max(prod_count);
        if max == 0 {
            self.workspace = WorkspaceContext::General;
        } else if dev_count == max {
            self.workspace = WorkspaceContext::Development;
        } else if comm_count == max {
            self.workspace = WorkspaceContext::Communication;
        } else if creative_count == max {
            self.workspace = WorkspaceContext::Creative;
        } else if prod_count == max {
            self.workspace = WorkspaceContext::Productivity;
        } else {
            self.workspace = WorkspaceContext::General;
        }
    }

    /// Get current workspace context.
    pub fn current_workspace(&self) -> &WorkspaceContext {
        &self.workspace
    }
}
