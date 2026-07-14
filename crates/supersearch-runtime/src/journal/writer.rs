//! Append-only journal writer.
//!
//! Writes journal entries to a segmented, append-only log file. Each segment
//! is a fixed-size file (default 64 MiB) that is memory-mapped for zero-copy
//! writes. When a segment fills, a new one is created and the old one is
//! sealed (made read-only).
//!
//! ## Durability
//! - Entries are written to the mmap region and `msync`'d on flush.
//! - The writer maintains a write-ahead position to enable crash recovery:
//!   on startup, scan from the last known good position and validate CRC32s.
//!
//! ## Concurrency
//! The writer is single-threaded (owned by the scheduler's journal task).
//! Producers send entries via an MPSC channel; the writer drains and serializes
//! sequentially, preserving the total ordering guarantee.

use std::path::{Path, PathBuf};

use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

use super::entry::JournalEntry;

/// Default segment size: 64 MiB.
const DEFAULT_SEGMENT_SIZE: usize = 64 * 1024 * 1024;

/// Errors from the journal writer.
#[derive(Debug, thiserror::Error)]
pub enum WriterError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("Segment full, rotation needed")]
    SegmentFull,
    #[error("Channel closed — no more producers")]
    ChannelClosed,
}

/// Metadata for a single journal segment file.
#[derive(Debug)]
struct Segment {
    /// Path to the segment file.
    path: PathBuf,
    /// Current write offset within the segment.
    write_offset: usize,
    /// Bytes already persisted to disk (via `flush`). Only `buffer[flushed_offset..write_offset]`
    /// needs to be written on the next flush — never the whole buffer, or flushing
    /// after every entry would cost O(n^2) disk I/O to fill one segment.
    flushed_offset: usize,
    /// Maximum size of this segment.
    capacity: usize,
    /// The backing buffer. In production this would be an mmap region;
    /// here we use a Vec for portability during initial development,
    /// with the interface designed for zero-copy mmap swap-in.
    buffer: Vec<u8>,
    /// Sequence number of the first entry in this segment.
    first_sequence: Option<u64>,
    /// Sequence number of the last entry written.
    last_sequence: Option<u64>,
    /// Whether this segment has been sealed (read-only).
    sealed: bool,
}

impl Segment {
    fn new(path: PathBuf, capacity: usize) -> Self {
        Self {
            path,
            write_offset: 0,
            flushed_offset: 0,
            capacity,
            buffer: Vec::with_capacity(capacity),
            first_sequence: None,
            last_sequence: None,
            sealed: false,
        }
    }

    /// Attempt to write an entry's serialized bytes into this segment.
    /// Returns `Err(WriterError::SegmentFull)` if there's insufficient space.
    fn write(&mut self, entry: &JournalEntry) -> Result<usize, WriterError> {
        let serialized = bincode::serialize(entry)
            .map_err(|e| WriterError::Serialization(e.to_string()))?;

        let entry_size = serialized.len();
        // 4-byte length prefix for framing.
        let total_size = 4 + entry_size;

        if self.write_offset + total_size > self.capacity {
            return Err(WriterError::SegmentFull);
        }

        // Write length-prefixed frame.
        self.buffer.extend_from_slice(&(entry_size as u32).to_le_bytes());
        self.buffer.extend_from_slice(&serialized);
        self.write_offset += total_size;

        // Track sequence range.
        let seq = entry.sequence.raw();
        if self.first_sequence.is_none() {
            self.first_sequence = Some(seq);
        }
        self.last_sequence = Some(seq);

        Ok(total_size)
    }

    fn seal(&mut self) {
        self.sealed = true;
        // In production: msync + chmod read-only.
    }

    /// Bytes left before this segment must rotate. Used by the (pending)
    /// segment-rotation path.
    #[allow(dead_code)]
    fn remaining(&self) -> usize {
        self.capacity.saturating_sub(self.write_offset)
    }
}

/// Handle for sending journal entries to the writer task.
#[derive(Clone)]
pub struct JournalSender {
    tx: mpsc::Sender<JournalEntry>,
}

impl JournalSender {
    /// Send an entry to the journal. Non-blocking; returns error if the
    /// writer's channel is full (backpressure signal).
    pub fn send(&self, entry: JournalEntry) -> Result<(), WriterError> {
        self.tx.try_send(entry).map_err(|_| WriterError::ChannelClosed)
    }

    /// Async send that waits for channel capacity.
    pub async fn send_async(&self, entry: JournalEntry) -> Result<(), WriterError> {
        self.tx.send(entry).await.map_err(|_| WriterError::ChannelClosed)
    }
}

/// The journal writer task. Consumes entries from an MPSC channel and
/// writes them to segmented log files.
pub struct JournalWriter {
    /// Directory containing journal segment files.
    journal_dir: PathBuf,
    /// Active (writable) segment.
    active_segment: Segment,
    /// Sealed segments (for compaction tracking).
    sealed_segments: Vec<PathBuf>,
    /// Segment counter for filename generation.
    segment_counter: u64,
    /// Segment capacity.
    segment_capacity: usize,
    /// Total entries written across all segments.
    total_entries: u64,
    /// Receive end of the entry channel.
    rx: mpsc::Receiver<JournalEntry>,
}

impl JournalWriter {
    /// Create a new journal writer and its corresponding sender handle.
    ///
    /// `channel_capacity`: buffer size for the MPSC channel. 4096 entries
    /// provides ~100ms of buffering at 40K entries/sec throughput.
    pub fn new(
        journal_dir: impl AsRef<Path>,
        channel_capacity: usize,
    ) -> (Self, JournalSender) {
        let journal_dir = journal_dir.as_ref().to_path_buf();
        let (tx, rx) = mpsc::channel(channel_capacity);

        let segment_path = journal_dir.join("segment_000000.journal");
        let active_segment = Segment::new(segment_path, DEFAULT_SEGMENT_SIZE);

        let writer = Self {
            journal_dir,
            active_segment,
            sealed_segments: Vec::new(),
            segment_counter: 0,
            segment_capacity: DEFAULT_SEGMENT_SIZE,
            total_entries: 0,
            rx,
        };

        (writer, JournalSender { tx })
    }

    /// Run the writer loop. Drains entries from the channel and writes to
    /// the active segment, rotating as needed.
    ///
    /// This should be spawned as a Tokio task at `Background` priority.
    pub async fn run(&mut self) -> Result<(), WriterError> {
        info!(dir = %self.journal_dir.display(), "Journal writer starting");

        // Ensure journal directory exists.
        tokio::fs::create_dir_all(&self.journal_dir).await?;

        loop {
            // Drain available entries in batches for throughput.
            let entry = match self.rx.recv().await {
                Some(e) => e,
                None => {
                    info!(total = self.total_entries, "Channel closed — writer shutting down");
                    self.flush().await?;
                    return Ok(());
                }
            };

            self.write_entry(entry).await?;

            // Drain any additional buffered entries without waiting.
            // This amortizes the write syscall overhead.
            let mut batch_count = 1u32;
            while let Ok(entry) = self.rx.try_recv() {
                self.write_entry(entry).await?;
                batch_count += 1;
                // Cap batch size to avoid starving other Background tasks.
                if batch_count >= 256 {
                    // Cooperative yield to the scheduler.
                    tokio::task::yield_now().await;
                    break;
                }
            }

            if batch_count > 1 {
                debug!(batch_count, "Batch write complete");
            }

            // Durability: persist the active segment whenever we catch up with
            // the channel. Without this, entries lived only in memory until a
            // 64 MiB rotation or shutdown — a crash lost the tail. Bounds loss
            // to at most the entries still in flight on the channel.
            if let Err(e) = self.flush().await {
                error!(error = %e, "Journal flush failed");
            }
        }
    }

    /// Write a single entry, rotating segments if needed.
    async fn write_entry(&mut self, entry: JournalEntry) -> Result<(), WriterError> {
        match self.active_segment.write(&entry) {
            Ok(_bytes) => {
                self.total_entries += 1;
                if self.total_entries.is_multiple_of(10_000) {
                    debug!(total = self.total_entries, bytes_used = self.active_segment.write_offset, "Journal progress");
                }
                Ok(())
            }
            Err(WriterError::SegmentFull) => {
                self.rotate_segment().await?;
                // Retry write in the new segment.
                self.active_segment.write(&entry).map_err(|e| {
                    error!("Write failed after rotation: {}", e);
                    e
                })?;
                self.total_entries += 1;
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// Seal the active segment and create a new one.
    async fn rotate_segment(&mut self) -> Result<(), WriterError> {
        // Persist any bytes not yet flushed before sealing.
        self.flush().await?;

        self.active_segment.seal();
        let sealed_path = self.active_segment.path.clone();
        info!(
            path = %sealed_path.display(),
            entries = self.active_segment.last_sequence.unwrap_or(0) -
                      self.active_segment.first_sequence.unwrap_or(0) + 1,
            "Segment sealed"
        );
        self.sealed_segments.push(sealed_path);

        // Create new segment.
        self.segment_counter += 1;
        let new_path = self.journal_dir.join(
            format!("segment_{:06}.journal", self.segment_counter)
        );
        self.active_segment = Segment::new(new_path, self.segment_capacity);

        Ok(())
    }

    /// Flush unpersisted bytes of the active segment to disk.
    ///
    /// Appends only the delta since the last flush (`buffer[flushed_offset..write_offset]`)
    /// rather than rewriting the whole segment buffer — the segment can grow up to
    /// `segment_capacity` (default 64 MiB), so re-writing it from byte 0 on every
    /// flush (which happens whenever the writer catches up with the channel, i.e.
    /// potentially after every single entry) would cost O(n^2) total disk I/O to
    /// fill one segment.
    async fn flush(&mut self) -> Result<(), WriterError> {
        let seg = &mut self.active_segment;
        if seg.write_offset > seg.flushed_offset {
            let delta = &seg.buffer[seg.flushed_offset..seg.write_offset];
            let mut file = tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&seg.path)
                .await?;
            file.write_all(delta).await?;
            file.flush().await?;
            let flushed_bytes = delta.len();
            seg.flushed_offset = seg.write_offset;
            info!(
                path = %seg.path.display(),
                bytes = flushed_bytes,
                total = seg.write_offset,
                "Flushed active segment"
            );
        }
        Ok(())
    }

    /// Returns the total number of entries written.
    pub fn total_entries(&self) -> u64 { self.total_entries }

    /// Returns paths to all sealed segments (for compaction).
    pub fn sealed_segments(&self) -> &[PathBuf] { &self.sealed_segments }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::entry::{EntryKind, JournalEntry};
    use crate::journal::reader::JournalReader;

    #[tokio::test]
    async fn entries_are_flushed_and_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let (mut writer, sender) = JournalWriter::new(dir.path(), 64);

        for i in 0..5u64 {
            sender
                .send(JournalEntry::new(
                    EntryKind::OsAutomationResult,
                    i,
                    "test".into(),
                    vec![i as u8],
                ))
                .unwrap();
        }
        // Close the channel so the writer drains, flushes, and returns.
        drop(sender);
        writer.run().await.unwrap();

        // A reader sees all entries persisted on disk (durability).
        let reader = JournalReader::new(dir.path(), true);
        let entries = reader.read_all().await.unwrap();
        assert_eq!(entries.len(), 5);
        assert!(entries.iter().all(|e| e.kind == EntryKind::OsAutomationResult));
    }
}
