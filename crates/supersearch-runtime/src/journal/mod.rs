//! # Module 2: Event Journal — Deterministic Replay System
//!
//! Every runtime action is appended to an immutable, append-only event journal.
//! To achieve absolute determinism, the system stores and replays literal token
//! streams and exact tool payloads, bypassing live LLM inference during replays.
//!
//! ## Guarantees
//! - **Append-only**: Entries are never mutated after write. CRC32 integrity.
//! - **Sequential consistency**: Entries are globally ordered by monotonic sequence numbers.
//! - **Replay fidelity**: The replay engine re-executes from journal entries with
//!   zero non-determinism — all external I/O is replaced with journaled payloads.
//!
//! ## Compaction
//! Old journal segments are compacted by retaining only checkpoint snapshots
//! and the entries since the last checkpoint. Compaction runs at `Background`
//! scheduler priority.

pub mod entry;
pub mod reader;
pub mod replay;
pub mod writer;

pub use entry::{EntryKind, JournalEntry, SequenceNumber};
pub use reader::JournalReader;
pub use replay::ReplayEngine;
pub use writer::JournalWriter;
