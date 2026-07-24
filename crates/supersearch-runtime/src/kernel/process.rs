//! Process management — supervised child process spawning.
//!
//! Plugins can request process spawning through the capability gate.
//! Spawned processes are supervised by the kernel's process manager,
//! which monitors their exit status and reports failures.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;
use tracing::info;

/// Configuration for a managed process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessConfig {
    /// Executable path.
    pub executable: PathBuf,
    /// Command-line arguments.
    pub args: Vec<String>,
    /// Environment variables (additive, not replacing).
    pub env: HashMap<String, String>,
    /// Working directory.
    pub working_dir: Option<PathBuf>,
    /// Whether to capture stdout.
    pub capture_stdout: bool,
    /// Whether to capture stderr.
    pub capture_stderr: bool,
    /// Maximum runtime before forced termination (None = unlimited).
    pub timeout: Option<std::time::Duration>,
    /// The plugin that requested this process spawn.
    pub owner_plugin: String,
}

/// A process managed by the kernel.
#[derive(Debug)]
pub struct ManagedProcess {
    /// Unique process ID assigned by the process manager.
    pub id: u64,
    /// Configuration used to spawn this process.
    pub config: ProcessConfig,
    /// OS process ID (PID).
    pub pid: Option<u32>,
    /// Current state.
    pub state: ProcessState,
    /// When the process was spawned.
    pub spawned_at: Instant,
    /// Captured stdout (if capture_stdout was true).
    pub stdout_buffer: Vec<u8>,
    /// Captured stderr (if capture_stderr was true).
    pub stderr_buffer: Vec<u8>,
    /// Exit code (if terminated).
    pub exit_code: Option<i32>,
}

/// Process lifecycle states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    /// Process has been created but not yet started.
    Created,
    /// Process is running.
    Running,
    /// Process has exited normally.
    Exited,
    /// Process was terminated (by signal or timeout).
    Terminated,
    /// Process failed to start.
    Failed,
}

/// Errors from process management.
#[derive(Debug, thiserror::Error)]
pub enum ProcessError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Process {0} not found")]
    NotFound(u64),
    #[error("Process {0} already terminated")]
    AlreadyTerminated(u64),
    #[error("Process limit reached: {current}/{max}")]
    LimitReached { current: usize, max: usize },
}

/// The process manager supervising all child processes.
pub struct ProcessManager {
    /// Active processes indexed by manager-assigned ID.
    processes: HashMap<u64, ManagedProcess>,
    /// Next process ID.
    next_id: u64,
    /// Maximum concurrent processes.
    max_processes: usize,
}

impl ProcessManager {
    pub fn new(max_processes: usize) -> Self {
        Self {
            processes: HashMap::new(),
            next_id: 0,
            max_processes,
        }
    }

    /// Spawn a new managed process.
    ///
    /// The caller must have already passed the capability gate check for
    /// `Permission::ProcessSpawn` in the appropriate namespace.
    pub fn spawn(&mut self, config: ProcessConfig) -> Result<u64, ProcessError> {
        if self.processes.len() >= self.max_processes {
            return Err(ProcessError::LimitReached {
                current: self.processes.len(),
                max: self.max_processes,
            });
        }

        let id = self.next_id;
        self.next_id += 1;

        // In production: use tokio::process::Command to spawn async.
        // let child = tokio::process::Command::new(&config.executable)
        //     .args(&config.args)
        //     .envs(&config.env)
        //     .current_dir(config.working_dir.as_ref().unwrap_or(&PathBuf::from(".")))
        //     .stdout(if config.capture_stdout { Stdio::piped() } else { Stdio::null() })
        //     .stderr(if config.capture_stderr { Stdio::piped() } else { Stdio::null() })
        //     .spawn()?;

        let process = ManagedProcess {
            id,
            config,
            pid: None, // Set after actual spawn.
            state: ProcessState::Created,
            spawned_at: Instant::now(),
            stdout_buffer: Vec::new(),
            stderr_buffer: Vec::new(),
            exit_code: None,
        };

        info!(
            process_id = id,
            executable = %process.config.executable.display(),
            owner = %process.config.owner_plugin,
            "Process spawned"
        );

        self.processes.insert(id, process);
        Ok(id)
    }

    /// Terminate a process by its manager ID.
    pub fn terminate(&mut self, id: u64) -> Result<(), ProcessError> {
        let process = self
            .processes
            .get_mut(&id)
            .ok_or(ProcessError::NotFound(id))?;

        if process.state == ProcessState::Exited || process.state == ProcessState::Terminated {
            return Err(ProcessError::AlreadyTerminated(id));
        }

        // In production: send SIGTERM, then SIGKILL after timeout.
        // if let Some(pid) = process.pid {
        //     unsafe { libc::kill(pid as i32, libc::SIGTERM); }
        // }

        process.state = ProcessState::Terminated;
        info!(process_id = id, "Process terminated");
        Ok(())
    }

    /// Terminate all processes owned by a specific plugin.
    /// Called during plugin unload.
    pub fn terminate_all_for_plugin(&mut self, plugin_id: &str) -> usize {
        let ids: Vec<u64> = self
            .processes
            .iter()
            .filter(|(_, p)| p.config.owner_plugin == plugin_id && p.state == ProcessState::Running)
            .map(|(id, _)| *id)
            .collect();

        let mut terminated = 0;
        for id in ids {
            if self.terminate(id).is_ok() {
                terminated += 1;
            }
        }

        if terminated > 0 {
            info!(
                plugin = plugin_id,
                count = terminated,
                "Terminated plugin processes"
            );
        }
        terminated
    }

    /// Get process state.
    pub fn get_state(&self, id: u64) -> Option<ProcessState> {
        self.processes.get(&id).map(|p| p.state)
    }

    /// Poll for completed processes and collect their exit statuses.
    /// Called periodically by the kernel.
    pub fn poll_completions(&mut self) -> Vec<(u64, i32)> {
        // In production: check JoinHandle results from tokio::process.
        Vec::new()
    }

    /// Number of active (running) processes.
    pub fn active_count(&self) -> usize {
        self.processes
            .values()
            .filter(|p| p.state == ProcessState::Running)
            .count()
    }

    /// Total processes ever managed.
    pub fn total_count(&self) -> usize {
        self.processes.len()
    }
}
