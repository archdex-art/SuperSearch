//! Task Planner — compiles `AgentIntent` into executable `TaskGraph`.
//!
//! The planner is the bridge between intent classification and execution.
//! It handles decomposition, dependency wiring, and platform-specific
//! command generation.

use tracing::debug;

use super::patterns::{AgentIntent, InfoKind, SystemCommand};
use super::task_graph::{NodeId, TaskGraph, TaskNodeKind};

/// Compiles classified intents into executable task graphs.
pub struct TaskPlanner;

impl Default for TaskPlanner {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskPlanner {
    pub fn new() -> Self {
        Self
    }

    /// Compile an intent into a task graph.
    pub fn plan(&self, intent: &AgentIntent) -> TaskGraph {
        match intent {
            AgentIntent::LaunchApp { app_name, args } => self.plan_launch_app(app_name, args),
            AgentIntent::OpenFile { path } => self.plan_open_file(path),
            AgentIntent::OpenUrl { url } => self.plan_open_url(url),
            AgentIntent::WebSearch { query } => self.plan_web_search(query),
            AgentIntent::FindFiles { query } => self.plan_find_files(query),
            AgentIntent::ClipboardRead => self.plan_clipboard_read(),
            AgentIntent::ClipboardWrite { content } => self.plan_clipboard_write(content),
            AgentIntent::SystemCommand { command } => self.plan_system_command(command),
            AgentIntent::ListRunningApps => self.plan_list_running_apps(),
            AgentIntent::SystemInfo { kind } => self.plan_system_info(kind),
            AgentIntent::MultiStep { intents } => self.plan_multi_step(intents),
            AgentIntent::QuitApp { app_name } => self.plan_quit_app(app_name),
            AgentIntent::SwitchApp { app_name } => self.plan_switch_app(app_name),
            AgentIntent::Unknown { raw_query } => self.plan_fallback(raw_query),
        }
    }

    fn plan_launch_app(&self, app_name: &str, args: &[String]) -> TaskGraph {
        let mut graph = TaskGraph::new(format!("Launch {}", app_name));
        graph.add_node(
            format!("Launch {}", app_name),
            TaskNodeKind::LaunchApp { app_name: app_name.to_string(), args: args.to_vec() },
        );
        debug!(app = app_name, "Planned: launch app");
        graph
    }

    fn plan_open_file(&self, path: &str) -> TaskGraph {
        let mut graph = TaskGraph::new(format!("Open {}", path));
        graph.add_node(
            format!("Open {}", path),
            TaskNodeKind::OpenFile { path: path.to_string() },
        );
        graph
    }

    fn plan_open_url(&self, url: &str) -> TaskGraph {
        let mut graph = TaskGraph::new(format!("Open {}", url));
        graph.add_node(
            format!("Open {}", url),
            TaskNodeKind::OpenUrl { url: url.to_string() },
        );
        graph
    }

    fn plan_web_search(&self, query: &str) -> TaskGraph {
        let mut graph = TaskGraph::new(format!("Web Search: {}", query));
        graph.add_node(
            format!("Search for \"{}\"", query),
            TaskNodeKind::WebSearch { query: query.to_string() },
        );
        graph
    }

    fn plan_find_files(&self, query: &str) -> TaskGraph {
        let mut graph = TaskGraph::new(format!("Find files: {}", query));
        graph.add_node(
            format!("Search for \"{}\"", query),
            TaskNodeKind::FindFiles { query: query.to_string() },
        );
        graph
    }

    fn plan_clipboard_read(&self) -> TaskGraph {
        let mut graph = TaskGraph::new("Read clipboard");
        graph.add_node("Read clipboard", TaskNodeKind::ClipboardRead);
        graph
    }

    fn plan_clipboard_write(&self, content: &str) -> TaskGraph {
        let mut graph = TaskGraph::new("Write to clipboard");
        graph.add_node(
            "Write to clipboard",
            TaskNodeKind::ClipboardWrite { content: content.to_string() },
        );
        graph
    }

    fn plan_system_command(&self, command: &SystemCommand) -> TaskGraph {
        let (label, script) = match command {
            SystemCommand::LockScreen => (
                "Lock Screen",
                r#"osascript -e 'tell application "System Events" to keystroke "q" using {command down, control down}'"#,
            ),
            SystemCommand::Sleep => (
                "Sleep",
                "pmset sleepnow",
            ),
            SystemCommand::DoNotDisturb => (
                "Toggle Do Not Disturb",
                r#"osascript -e 'tell application "System Events" to keystroke "d" using {command down, shift down, option down}'"#,
            ),
            SystemCommand::VolumeUp => (
                "Volume Up",
                "osascript -e 'set volume output volume ((output volume of (get volume settings)) + 10)'",
            ),
            SystemCommand::VolumeDown => (
                "Volume Down",
                "osascript -e 'set volume output volume ((output volume of (get volume settings)) - 10)'",
            ),
            SystemCommand::VolumeMute => (
                "Toggle Mute",
                "osascript -e 'set volume output muted (not (output muted of (get volume settings)))'",
            ),
            SystemCommand::BrightnessUp => (
                "Brightness Up",
                r#"osascript -e 'tell application "System Events" to key code 144'"#,
            ),
            SystemCommand::BrightnessDown => (
                "Brightness Down",
                r#"osascript -e 'tell application "System Events" to key code 145'"#,
            ),
            SystemCommand::EmptyTrash => (
                "Empty Trash",
                r#"osascript -e 'tell application "Finder" to empty trash'"#,
            ),
            SystemCommand::Screenshot => (
                "Take Screenshot",
                "screencapture -i ~/Desktop/screenshot.png",
            ),
            SystemCommand::ShowDesktop => (
                "Show Desktop",
                r#"osascript -e 'tell application "System Events" to key code 103'"#,
            ),
            SystemCommand::ToggleDarkMode => (
                "Toggle Dark Mode",
                r#"osascript -e 'tell app "System Events" to tell appearance preferences to set dark mode to not dark mode'"#,
            ),
            SystemCommand::Restart => (
                "Restart",
                r#"osascript -e 'tell app "System Events" to restart'"#,
            ),
            SystemCommand::Shutdown => (
                "Shutdown",
                r#"osascript -e 'tell app "System Events" to shut down'"#,
            ),
        };

        let mut graph = TaskGraph::new(label);
        graph.add_node(
            label,
            TaskNodeKind::SystemCommand {
                script: script.to_string(),
                label: label.to_string(),
            },
        );
        graph
    }

    fn plan_list_running_apps(&self) -> TaskGraph {
        let mut graph = TaskGraph::new("List running apps");
        graph.add_node("List running apps", TaskNodeKind::ListRunningApps);
        graph
    }

    fn plan_system_info(&self, kind: &InfoKind) -> TaskGraph {
        let (label, command) = match kind {
            InfoKind::DiskSpace => ("Disk Space", "df -h / | tail -1"),
            InfoKind::Battery => ("Battery Status", "pmset -g batt"),
            InfoKind::Memory => ("Memory Usage", "vm_stat | head -5"),
            InfoKind::Cpu => ("CPU Usage", "top -l 1 -n 0 | head -10"),
            InfoKind::Network => ("Network Info", "ifconfig en0 | grep inet"),
            InfoKind::Uptime => ("System Uptime", "uptime"),
            InfoKind::General => ("System Info", "sw_vers && sysctl -n hw.memsize && df -h / | tail -1"),
        };

        let mut graph = TaskGraph::new(label);
        graph.add_node(
            label,
            TaskNodeKind::SystemInfo {
                command: command.to_string(),
                label: label.to_string(),
            },
        );
        graph
    }

    fn plan_quit_app(&self, app_name: &str) -> TaskGraph {
        let mut graph = TaskGraph::new(format!("Quit {}", app_name));
        graph.add_node(
            format!("Quit {}", app_name),
            TaskNodeKind::QuitApp { app_name: app_name.to_string() },
        );
        graph
    }

    fn plan_switch_app(&self, app_name: &str) -> TaskGraph {
        let mut graph = TaskGraph::new(format!("Switch to {}", app_name));
        graph.add_node(
            format!("Switch to {}", app_name),
            TaskNodeKind::SwitchApp { app_name: app_name.to_string() },
        );
        graph
    }

    fn plan_multi_step(&self, intents: &[AgentIntent]) -> TaskGraph {
        let descriptions: Vec<String> = intents
            .iter()
            .map(|i| self.intent_description(i))
            .collect();
        let mut graph = TaskGraph::new(descriptions.join(", "));

        // For multi-step, each step depends on the previous (sequential by
        // default). A step's own sub-graph may carry internal dependency
        // edges (e.g. a future multi-node plan) — those are remapped and
        // preserved rather than discarded, so the flattened graph never
        // silently loses a dependency the sub-plan required.
        let mut prev_last_id: Option<NodeId> = None;
        for intent in intents {
            let sub_graph = self.plan(intent);
            if sub_graph.nodes.is_empty() {
                continue;
            }

            let id_map: Vec<NodeId> = sub_graph
                .nodes
                .iter()
                .map(|node| graph.add_node(node.label.clone(), node.kind.clone()))
                .collect();

            for edge in &sub_graph.edges {
                graph.add_edge(id_map[edge.prerequisite], id_map[edge.dependent]);
            }

            if let Some(prev) = prev_last_id {
                graph.add_edge(prev, id_map[0]);
            }
            prev_last_id = id_map.last().copied();
        }

        graph
    }

    fn plan_fallback(&self, query: &str) -> TaskGraph {
        let mut graph = TaskGraph::new(format!("Search: {}", query));
        graph.add_node(
            format!("Search for \"{}\"", query),
            TaskNodeKind::FindFiles { query: query.to_string() },
        );
        graph
    }

    /// Get a human-readable description of an intent.
    fn intent_description(&self, intent: &AgentIntent) -> String {
        match intent {
            AgentIntent::LaunchApp { app_name, .. } => format!("Launch {}", app_name),
            AgentIntent::OpenFile { path } => format!("Open {}", path),
            AgentIntent::OpenUrl { url } => format!("Open {}", url),
            AgentIntent::WebSearch { query } => format!("Search Web for \"{}\"", query),
            AgentIntent::FindFiles { query } => format!("Find {}", query),
            AgentIntent::ClipboardRead => "Read clipboard".into(),
            AgentIntent::ClipboardWrite { .. } => "Copy to clipboard".into(),
            AgentIntent::SystemCommand { command } => format!("{:?}", command),
            AgentIntent::ListRunningApps => "List running apps".into(),
            AgentIntent::SystemInfo { kind } => format!("{:?} info", kind),
            AgentIntent::MultiStep { intents } => format!("{} steps", intents.len()),
            AgentIntent::QuitApp { app_name } => format!("Quit {}", app_name),
            AgentIntent::SwitchApp { app_name } => format!("Switch to {}", app_name),
            AgentIntent::Unknown { raw_query } => format!("Search: {}", raw_query),
        }
    }
}
