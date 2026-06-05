//! Journal entry data structures.
//!
//! Each entry is a self-contained record of a single runtime action,
//! carrying the full payload needed for deterministic replay.

use serde::{Serialize, Deserialize};
use std::sync::atomic::{AtomicU64, Ordering};


/// Monotonically increasing, gap-free sequence number.
/// Provides total ordering of all journal entries across the runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SequenceNumber(pub u64);

impl SequenceNumber {
    /// Allocate the next sequence number. Wait-free on x86_64.
    #[inline]
    pub fn next() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        SequenceNumber(COUNTER.fetch_add(1, Ordering::Relaxed))
    }

    #[inline]
    pub const fn raw(self) -> u64 { self.0 }
}

impl std::fmt::Display for SequenceNumber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "seq#{}", self.0)
    }
}

/// Classification of journal entry types.
///
/// The replay engine uses this to determine how to re-execute each entry:
/// - Deterministic entries (TaskSpawn, TaskComplete) are re-executed directly.
/// - Non-deterministic entries (LlmInference, ExternalIo) are replayed from
///   their stored payloads, bypassing live execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum EntryKind {
    // ─── Scheduler Events ────────────────────────────────────────────
    /// A task was spawned into the scheduler.
    TaskSpawn = 0,
    /// A task completed (success or cancellation).
    TaskComplete = 1,
    /// A task was promoted due to starvation aging.
    TaskPromoted = 2,
    /// Scheduler tick boundary marker.
    TickBoundary = 3,

    // ─── Capability Events ───────────────────────────────────────────
    /// A capability was granted to a plugin.
    CapabilityGranted = 10,
    /// A capability was revoked from a plugin.
    CapabilityRevoked = 11,
    /// A capability gate check (allowed or denied).
    CapabilityCheck = 12,

    // ─── Plugin Events ───────────────────────────────────────────────
    /// A plugin was loaded into the sandbox.
    PluginLoaded = 20,
    /// A plugin was unloaded.
    PluginUnloaded = 21,
    /// An IPC message was sent between kernel and plugin.
    PluginIpcMessage = 22,

    // ─── AI / LLM Events (NON-DETERMINISTIC — replayed from payload) ─
    /// An LLM inference request. During replay, the stored token stream
    /// is used instead of calling the live model.
    LlmInferenceRequest = 30,
    /// The complete LLM response (literal token stream).
    LlmInferenceResponse = 31,
    /// A tool call payload from an AI agent.
    ToolCallPayload = 32,
    /// The tool call result.
    ToolCallResult = 33,

    // ─── External I/O (NON-DETERMINISTIC — replayed from payload) ────
    /// File system read result.
    FsReadResult = 40,
    /// Network response.
    NetworkResponse = 41,
    /// OS automation action result.
    OsAutomationResult = 42,

    // ─── Reactive Graph Events ───────────────────────────────────────
    /// A signal value changed.
    SignalUpdate = 50,
    /// A fast-path bypass was applied.
    FastPathApplied = 51,
    /// A reconciliation completed.
    ReconciliationComplete = 52,

    // ─── Checkpoints ─────────────────────────────────────────────────
    /// Full state snapshot for compaction. Replay can start from any
    /// checkpoint instead of replaying from the beginning.
    Checkpoint = 255,
}

impl EntryKind {
    /// Returns true if this entry type requires payload replay (non-deterministic).
    /// During replay, these entries use their stored payloads instead of
    /// executing live operations.
    #[inline]
    pub const fn is_non_deterministic(self) -> bool {
        matches!(
            self,
            EntryKind::LlmInferenceRequest
                | EntryKind::LlmInferenceResponse
                | EntryKind::ToolCallPayload
                | EntryKind::ToolCallResult
                | EntryKind::FsReadResult
                | EntryKind::NetworkResponse
                | EntryKind::OsAutomationResult
        )
    }
}

/// A single journal entry. Immutable after creation.
///
/// ## Wire Format (for on-disk serialization)
/// ```text
/// ┌──────────┬──────┬───────────┬──────────┬────────────┬──────────┐
/// │ seq (8B) │ kind │ ts_ns(8B) │ len (4B) │ payload    │ crc (4B) │
/// │          │ (1B) │           │          │ (variable) │          │
/// └──────────┴──────┴───────────┴──────────┴────────────┴──────────┘
/// ```
///
/// Total overhead: 25 bytes per entry (excluding payload).
/// CRC32 covers all preceding fields including payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    /// Global sequence number. Gap-free, monotonically increasing.
    pub sequence: SequenceNumber,

    /// Classification of this entry.
    pub kind: EntryKind,

    /// Nanoseconds since runtime boot (monotonic clock).
    /// NOT wall-clock time — deterministic across replays.
    pub timestamp_ns: u64,

    /// The entry origin (module/plugin name).
    pub origin: String,

    /// Opaque payload. For non-deterministic entries, this contains the
    /// literal response data used during replay. For deterministic entries,
    /// this carries metadata for tracing/debugging.
    ///
    /// Zero-copy note: In the hot path, we use `Bytes` (reference-counted)
    /// to avoid copying payloads between the journal writer and consumers.
    /// The `Vec<u8>` here is for the serde boundary; the writer internally
    /// works with slices.
    pub payload: Vec<u8>,

    /// CRC32 checksum of all preceding fields. Validated on read.
    pub checksum: u32,
}

impl JournalEntry {
    /// Create a new journal entry, computing the CRC32 checksum.
    pub fn new(kind: EntryKind, timestamp_ns: u64, origin: String, payload: Vec<u8>) -> Self {
        let sequence = SequenceNumber::next();
        let mut entry = Self {
            sequence,
            kind,
            timestamp_ns,
            origin,
            payload,
            checksum: 0, // computed below
        };
        entry.checksum = entry.compute_checksum();
        entry
    }

    /// Compute CRC32 over all fields except the checksum itself.
    pub fn compute_checksum(&self) -> u32 {
        let mut hasher = crc32fast::Hasher::new();
        hasher.update(&self.sequence.0.to_le_bytes());
        hasher.update(&[self.kind as u8]);
        hasher.update(&self.timestamp_ns.to_le_bytes());
        hasher.update(self.origin.as_bytes());
        hasher.update(&self.payload);
        hasher.finalize()
    }

    /// Validate the entry's integrity.
    #[inline]
    pub fn is_valid(&self) -> bool {
        self.checksum == self.compute_checksum()
    }

    /// Serialized size in bytes (for pre-allocation).
    pub fn wire_size(&self) -> usize {
        // seq(8) + kind(1) + ts(8) + origin_len(4) + origin + payload_len(4) + payload + crc(4)
        8 + 1 + 8 + 4 + self.origin.len() + 4 + self.payload.len() + 4
    }
}
