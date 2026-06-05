//! System context engine — tracks active apps, recent files, workspace state.
//!
//! Provides runtime context signals for search ranking, intent disambiguation,
//! and proactive suggestions. Updated periodically by the kernel.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::Instant;

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

impl ContextEngine {
    pub fn new() -> Self {
        Self {
            active_app: None,
            recent_apps: VecDeque::with_capacity(MAX_RECENT),
            recent_files: VecDeque::with_capacity(MAX_RECENT),
            workspace: WorkspaceContext::General,
            last_update: Instant::now(),
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
        let dev_apps = ["code", "terminal", "iterm", "xcode", "intellij", "neovim", "warp", "cursor"];
        let comm_apps = ["slack", "teams", "discord", "mail", "zoom", "messages"];
        let creative_apps = ["figma", "photoshop", "illustrator", "final cut", "premiere", "sketch"];
        let prod_apps = ["pages", "numbers", "keynote", "word", "excel", "notes", "notion"];

        let recent: Vec<String> = self.recent_apps.iter().take(5).map(|a| a.to_lowercase()).collect();

        let dev_count = recent.iter().filter(|a| dev_apps.iter().any(|d| a.contains(d))).count();
        let comm_count = recent.iter().filter(|a| comm_apps.iter().any(|d| a.contains(d))).count();
        let creative_count = recent.iter().filter(|a| creative_apps.iter().any(|d| a.contains(d))).count();
        let prod_count = recent.iter().filter(|a| prod_apps.iter().any(|d| a.contains(d))).count();

        let max = dev_count.max(comm_count).max(creative_count).max(prod_count);
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
