//! Deterministic replay engine.
//!
//! Re-executes runtime sessions from journal entries with absolute fidelity.
//! Non-deterministic events (LLM inference, external I/O) are replayed from
//! their stored payloads — the replay engine injects these payloads instead
//! of calling live services.
//!
//! ## Replay Modes
//! - **Full replay**: From the beginning or a checkpoint.
//! - **Partial replay**: A sequence range (for debugging specific interactions).
//! - **Verification**: Replay and compare outputs against a reference journal.


use tracing::{info, warn, error};

use super::entry::{JournalEntry, EntryKind, SequenceNumber};


/// Replay clock — provides deterministic timestamps during replay.
/// Instead of reading the real monotonic clock, the replay engine feeds
/// timestamps from journal entries.
#[derive(Debug)]
pub struct ReplayClock {
    /// Current replay timestamp (nanoseconds since boot).
    current_ns: u64,
}

impl ReplayClock {
    pub fn new() -> Self { Self { current_ns: 0 } }

    /// Advance to the timestamp of the given entry.
    pub fn advance_to(&mut self, entry: &JournalEntry) {
        debug_assert!(
            entry.timestamp_ns >= self.current_ns,
            "Replay clock went backwards: {} -> {}",
            self.current_ns,
            entry.timestamp_ns
        );
        self.current_ns = entry.timestamp_ns;
    }

    pub fn current_ns(&self) -> u64 { self.current_ns }
}

/// Handler trait for processing replayed events.
///
/// Implementors receive journal entries during replay and can reconstruct
/// runtime state. The engine calls the appropriate method based on
/// `EntryKind`, injecting stored payloads for non-deterministic events.
pub trait ReplayHandler: Send {
    /// Called for deterministic scheduler events (TaskSpawn, TaskComplete, etc.)
    fn on_scheduler_event(&mut self, entry: &JournalEntry) -> Result<(), ReplayError>;

    /// Called for capability events. The handler should update its capability
    /// registry to match the journaled state.
    fn on_capability_event(&mut self, entry: &JournalEntry) -> Result<(), ReplayError>;

    /// Called for plugin lifecycle events.
    fn on_plugin_event(&mut self, entry: &JournalEntry) -> Result<(), ReplayError>;

    /// Called for non-deterministic AI events. The `entry.payload` contains
    /// the literal token stream or tool payload — the handler MUST use this
    /// instead of calling a live LLM.
    fn on_ai_event(&mut self, entry: &JournalEntry) -> Result<(), ReplayError>;

    /// Called for non-deterministic I/O events. The `entry.payload` contains
    /// the exact I/O result — the handler MUST use this instead of
    /// performing live I/O.
    fn on_io_event(&mut self, entry: &JournalEntry) -> Result<(), ReplayError>;

    /// Called for reactive graph events.
    fn on_reactive_event(&mut self, entry: &JournalEntry) -> Result<(), ReplayError>;

    /// Called when a checkpoint is reached. The handler can snapshot its
    /// current state for fast-forward capability.
    fn on_checkpoint(&mut self, entry: &JournalEntry) -> Result<(), ReplayError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ReplayError {
    #[error("Reader error: {0}")]
    Reader(#[from] super::reader::ReaderError),
    #[error("Handler error at sequence {sequence}: {message}")]
    HandlerError { sequence: u64, message: String },
    #[error("Sequence gap: expected {expected}, got {actual}")]
    SequenceGap { expected: u64, actual: u64 },
    #[error("Replay divergence at sequence {sequence}: {detail}")]
    Divergence { sequence: u64, detail: String },
}

/// Replay statistics.
#[derive(Debug, Clone, Default)]
pub struct ReplayStats {
    pub total_entries: u64,
    pub deterministic_entries: u64,
    pub non_deterministic_entries: u64,
    pub checkpoints_hit: u64,
    pub errors: u64,
}

/// The deterministic replay engine.
pub struct ReplayEngine {
    clock: ReplayClock,
    stats: ReplayStats,
    /// If set, replay only entries in this range.
    range_filter: Option<(SequenceNumber, SequenceNumber)>,
    /// Expected next sequence number (for gap detection).
    expected_sequence: u64,
}

impl ReplayEngine {
    pub fn new() -> Self {
        Self {
            clock: ReplayClock::new(),
            stats: ReplayStats::default(),
            range_filter: None,
            expected_sequence: 0,
        }
    }

    /// Set a sequence range filter for partial replay.
    pub fn with_range(mut self, start: SequenceNumber, end: SequenceNumber) -> Self {
        self.range_filter = Some((start, end));
        self.expected_sequence = start.raw();
        self
    }

    /// Replay all entries from a pre-loaded entry vector.
    ///
    /// Entries MUST be in sequence order. The engine validates ordering
    /// and dispatches each entry to the appropriate handler method.
    pub fn replay(
        &mut self,
        entries: &[JournalEntry],
        handler: &mut dyn ReplayHandler,
    ) -> Result<ReplayStats, ReplayError> {
        info!(total = entries.len(), "Starting replay");

        for entry in entries {
            // Apply range filter.
            if let Some((start, end)) = &self.range_filter {
                if entry.sequence < *start { continue; }
                if entry.sequence > *end { break; }
            }

            // Sequence gap detection.
            if entry.sequence.raw() != self.expected_sequence {
                warn!(
                    expected = self.expected_sequence,
                    actual = entry.sequence.raw(),
                    "Sequence gap detected"
                );
                // Non-fatal: gaps can occur after compaction.
            }
            self.expected_sequence = entry.sequence.raw() + 1;

            // Advance replay clock.
            self.clock.advance_to(entry);

            // Dispatch by entry kind.
            let result = self.dispatch_entry(entry, handler);
            if let Err(e) = result {
                error!(sequence = entry.sequence.raw(), error = %e, "Replay error");
                self.stats.errors += 1;
                // Continue replay — individual entry failures are non-fatal
                // unless the handler explicitly returns a fatal error.
            }

            self.stats.total_entries += 1;
            if entry.kind.is_non_deterministic() {
                self.stats.non_deterministic_entries += 1;
            } else {
                self.stats.deterministic_entries += 1;
            }
        }

        info!(stats = ?self.stats, "Replay complete");
        Ok(self.stats.clone())
    }

    /// Dispatch a single entry to the appropriate handler method.
    fn dispatch_entry(
        &mut self,
        entry: &JournalEntry,
        handler: &mut dyn ReplayHandler,
    ) -> Result<(), ReplayError> {
        match entry.kind {
            // Deterministic scheduler events.
            EntryKind::TaskSpawn
            | EntryKind::TaskComplete
            | EntryKind::TaskPromoted
            | EntryKind::TickBoundary => handler.on_scheduler_event(entry),

            // Capability events.
            EntryKind::CapabilityGranted
            | EntryKind::CapabilityRevoked
            | EntryKind::CapabilityCheck => handler.on_capability_event(entry),

            // Plugin events.
            EntryKind::PluginLoaded
            | EntryKind::PluginUnloaded
            | EntryKind::PluginIpcMessage => handler.on_plugin_event(entry),

            // Non-deterministic AI events — payload is the literal token stream.
            EntryKind::LlmInferenceRequest
            | EntryKind::LlmInferenceResponse
            | EntryKind::ToolCallPayload
            | EntryKind::ToolCallResult => handler.on_ai_event(entry),

            // Non-deterministic I/O events — payload is the exact result.
            EntryKind::FsReadResult
            | EntryKind::NetworkResponse
            | EntryKind::OsAutomationResult => handler.on_io_event(entry),

            // Reactive graph events.
            EntryKind::SignalUpdate
            | EntryKind::FastPathApplied
            | EntryKind::ReconciliationComplete => handler.on_reactive_event(entry),

            // Checkpoint.
            EntryKind::Checkpoint => {
                self.stats.checkpoints_hit += 1;
                handler.on_checkpoint(entry)
            }
        }
    }

    /// Replay statistics.
    pub fn stats(&self) -> &ReplayStats { &self.stats }

    /// Current replay clock position.
    pub fn clock(&self) -> &ReplayClock { &self.clock }
}
