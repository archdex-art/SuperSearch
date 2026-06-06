//! Journal reader for sequential and random-access replay.
//!
//! Reads length-prefixed, bincode-serialized journal entries from segment
//! files. Validates CRC32 integrity on each entry and supports:
//! - Sequential forward scan (for full replay)
//! - Checkpoint-seeking (skip to nearest checkpoint, then replay forward)
//! - Filtered iteration (by EntryKind or sequence range)

use std::path::{Path, PathBuf};
use tracing::{debug, warn};

use super::entry::{JournalEntry, EntryKind, SequenceNumber};

#[derive(Debug, thiserror::Error)]
pub enum ReaderError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Deserialization error at offset {offset}: {message}")]
    Deserialization { offset: usize, message: String },
    #[error("CRC32 mismatch at sequence {sequence}: expected {expected:#010x}, got {actual:#010x}")]
    ChecksumMismatch { sequence: u64, expected: u32, actual: u32 },
    #[error("Truncated entry at offset {offset}: need {need} bytes, have {have}")]
    Truncated { offset: usize, need: usize, have: usize },
}

/// Iterator over journal entries in a single segment.
pub struct SegmentIterator {
    data: Vec<u8>,
    offset: usize,
    /// Source segment path, retained for diagnostics/error context on corrupt
    /// reads. Not yet surfaced in returned errors.
    #[allow(dead_code)]
    segment_path: PathBuf,
    entries_read: u64,
    validate_checksums: bool,
}

impl SegmentIterator {
    /// Load a segment file into memory and prepare for iteration.
    pub async fn open(path: impl AsRef<Path>, validate_checksums: bool) -> Result<Self, ReaderError> {
        let path = path.as_ref().to_path_buf();
        let data = tokio::fs::read(&path).await?;
        debug!(path = %path.display(), bytes = data.len(), "Loaded journal segment");

        Ok(Self {
            data,
            offset: 0,
            segment_path: path,
            entries_read: 0,
            validate_checksums,
        })
    }

    /// Read the next entry from the segment.
    pub fn next_entry(&mut self) -> Result<Option<JournalEntry>, ReaderError> {
        if self.offset >= self.data.len() {
            return Ok(None);
        }

        // Read 4-byte length prefix.
        if self.offset + 4 > self.data.len() {
            return Err(ReaderError::Truncated {
                offset: self.offset,
                need: 4,
                have: self.data.len() - self.offset,
            });
        }
        let len_bytes: [u8; 4] = self.data[self.offset..self.offset + 4]
            .try_into()
            .unwrap();
        let entry_len = u32::from_le_bytes(len_bytes) as usize;
        self.offset += 4;

        // Read entry payload.
        if self.offset + entry_len > self.data.len() {
            return Err(ReaderError::Truncated {
                offset: self.offset,
                need: entry_len,
                have: self.data.len() - self.offset,
            });
        }

        let entry_bytes = &self.data[self.offset..self.offset + entry_len];
        let entry: JournalEntry = bincode::deserialize(entry_bytes)
            .map_err(|e| ReaderError::Deserialization {
                offset: self.offset,
                message: e.to_string(),
            })?;
        self.offset += entry_len;

        // CRC32 validation.
        if self.validate_checksums {
            let computed = entry.compute_checksum();
            if computed != entry.checksum {
                return Err(ReaderError::ChecksumMismatch {
                    sequence: entry.sequence.raw(),
                    expected: entry.checksum,
                    actual: computed,
                });
            }
        }

        self.entries_read += 1;
        Ok(Some(entry))
    }

    /// Collect all entries in this segment.
    pub fn collect_all(&mut self) -> Result<Vec<JournalEntry>, ReaderError> {
        let mut entries = Vec::new();
        while let Some(entry) = self.next_entry()? {
            entries.push(entry);
        }
        Ok(entries)
    }

    /// Number of entries read so far.
    pub fn entries_read(&self) -> u64 { self.entries_read }
}

/// Multi-segment journal reader that iterates across all segments in order.
pub struct JournalReader {
    journal_dir: PathBuf,
    validate_checksums: bool,
}

impl JournalReader {
    pub fn new(journal_dir: impl AsRef<Path>, validate_checksums: bool) -> Self {
        Self {
            journal_dir: journal_dir.as_ref().to_path_buf(),
            validate_checksums,
        }
    }

    /// Discover and sort all segment files in the journal directory.
    pub async fn discover_segments(&self) -> Result<Vec<PathBuf>, ReaderError> {
        let mut segments = Vec::new();
        let mut dir = tokio::fs::read_dir(&self.journal_dir).await?;

        while let Some(entry) = dir.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("journal") {
                segments.push(path);
            }
        }

        // Sort by filename (which encodes segment order).
        segments.sort();
        debug!(count = segments.len(), "Discovered journal segments");
        Ok(segments)
    }

    /// Read all entries across all segments, in sequence order.
    pub async fn read_all(&self) -> Result<Vec<JournalEntry>, ReaderError> {
        let segments = self.discover_segments().await?;
        let mut all_entries = Vec::new();

        for seg_path in &segments {
            let mut iter = SegmentIterator::open(seg_path, self.validate_checksums).await?;
            let entries = iter.collect_all()?;
            all_entries.extend(entries);
        }

        // Verify total ordering.
        for window in all_entries.windows(2) {
            if window[1].sequence.raw() <= window[0].sequence.raw() {
                warn!(
                    "Sequence ordering violation: {} followed by {}",
                    window[0].sequence, window[1].sequence
                );
            }
        }

        Ok(all_entries)
    }

    /// Find the last checkpoint entry and return its index in the entry list.
    /// Replay can start from this checkpoint instead of the beginning.
    pub fn find_last_checkpoint(entries: &[JournalEntry]) -> Option<usize> {
        entries.iter().rposition(|e| e.kind == EntryKind::Checkpoint)
    }

    /// Filter entries by kind.
    pub fn filter_by_kind(entries: &[JournalEntry], kind: EntryKind) -> Vec<&JournalEntry> {
        entries.iter().filter(|e| e.kind == kind).collect()
    }

    /// Filter entries by sequence range (inclusive).
    pub fn filter_by_range(
        entries: &[JournalEntry],
        start: SequenceNumber,
        end: SequenceNumber,
    ) -> Vec<&JournalEntry> {
        entries.iter()
            .filter(|e| e.sequence >= start && e.sequence <= end)
            .collect()
    }
}
