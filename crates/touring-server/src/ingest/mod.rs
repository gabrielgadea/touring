//! Live JSONL ingestion module
//!
//! Watches JSONL files for new lines and ingests them into memory
//! with auto-embedding via GPU service.

pub mod parser;
pub mod transcript_miner;
pub mod watcher;

pub use parser::{JsonlParser, ParsedEntry, SourceType};
pub use transcript_miner::{
    ContentBlock, ERROR_TEXT_MAX, ErrorResolutionPair, MinerSweepStats, ParsedTranscriptLine,
    RESOLUTION_SCAN_WINDOW, TranscriptMiner, TranscriptRole, dedup_key, discover_transcript_paths,
    extract_error_resolution_pairs, parse_transcript_line,
};
pub use watcher::JsonlWatcher;
