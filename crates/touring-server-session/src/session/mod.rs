//! Session Subsystem - Session lifecycle with auto-assessment
//!
//! Manages conversation/task sessions with metrics tracking and auto-assessment.

pub mod manager;

pub use manager::{Session, SessionCheckpoint, SessionError, SessionManager, SessionStatus};
