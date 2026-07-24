//! Learning Signals Bridge — connects ActorCritic to AgenticRL without circular deps
//!
//! Uses closure injection: touring-hooks registers callbacks during init.
//! ActorCritic calls emit_advantage/emit_td_error which invoke the registered callbacks.

use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// Advantage signal from ActorCritic policy gradient update
#[derive(Debug, Clone)]
pub struct ActorAdvantage {
    /// State hash the advantage was computed for.
    pub state: u64,
    /// Action index the advantage applies to.
    pub action: usize,
    /// Advantage estimate from the policy-gradient update.
    pub advantage: f32,
    /// Emission time as milliseconds since the Unix epoch.
    pub timestamp: u64,
}

/// TD-Error signal for value function baseline
#[derive(Debug, Clone)]
pub struct TdErrorSignal {
    /// Temporal-difference error (target minus baseline).
    pub td_error: f32,
    /// Value-function estimate for the current state.
    pub v_s: f32,
    /// Value-function estimate for the next state.
    pub v_s_next: f32,
    /// Emission time as milliseconds since the Unix epoch.
    pub timestamp: u64,
}

// ── Signal Emitters (called by ActorCritic internally) ─────────────────────

type AdvantageCallback = Box<dyn Fn(ActorAdvantage) + Send + Sync>;
type TdErrorCallback = Box<dyn Fn(TdErrorSignal) + Send + Sync>;

static ADVANTAGE_HANDLERS: RwLock<Vec<AdvantageCallback>> = RwLock::new(Vec::new());
static TD_ERROR_HANDLERS: RwLock<Vec<TdErrorCallback>> = RwLock::new(Vec::new());

/// Register a handler for ActorAdvantage signals
pub fn on_actor_advantage<F>(handler: F)
where
    F: Fn(ActorAdvantage) + Send + Sync + 'static,
{
    ADVANTAGE_HANDLERS
        .write()
        .expect("advantage handlers poisoned")
        .push(Box::new(handler));
}

/// Register a handler for TD-Error signals
pub fn on_td_error<F>(handler: F)
where
    F: Fn(TdErrorSignal) + Send + Sync + 'static,
{
    TD_ERROR_HANDLERS
        .write()
        .expect("td_error handlers poisoned")
        .push(Box::new(handler));
}

/// Emit an ActorAdvantage signal to all registered handlers
pub fn emit_advantage(signal: ActorAdvantage) {
    let handlers = ADVANTAGE_HANDLERS
        .read()
        .expect("advantage handlers poisoned");
    for handler in handlers.iter() {
        handler(signal.clone());
    }
}

/// Emit a TdErrorSignal to all registered handlers
pub fn emit_td_error(signal: TdErrorSignal) {
    let handlers = TD_ERROR_HANDLERS
        .read()
        .expect("td_error handlers poisoned");
    for handler in handlers.iter() {
        handler(signal.clone());
    }
}

/// Current unix timestamp in millis
pub fn timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time before epoch")
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_on_actor_advantage_and_emit() {
        use std::sync::{Arc, Mutex};
        let received = Arc::new(Mutex::new(None));
        let received_clone = Arc::clone(&received);
        on_actor_advantage(move |sig| {
            *received_clone.lock().unwrap() = Some(sig);
        });
        let signal = ActorAdvantage {
            state: 1,
            action: 42,
            advantage: 0.95,
            timestamp: timestamp_ms(),
        };
        emit_advantage(signal.clone());
        let r = received.lock().unwrap().take().unwrap();
        assert_eq!(r.state, 1);
        assert_eq!(r.action, 42);
        assert!((r.advantage - 0.95).abs() < f32::EPSILON);
    }

    #[test]
    fn test_on_td_error_and_emit() {
        use std::sync::{Arc, Mutex};
        let received = Arc::new(Mutex::new(None));
        let received_clone = Arc::clone(&received);
        on_td_error(move |sig| {
            *received_clone.lock().unwrap() = Some(sig);
        });
        let signal = TdErrorSignal {
            td_error: 0.3,
            v_s: 1.2,
            v_s_next: 1.5,
            timestamp: timestamp_ms(),
        };
        emit_td_error(signal.clone());
        let r = received.lock().unwrap().take().unwrap();
        assert!((r.td_error - 0.3).abs() < f32::EPSILON);
        assert!((r.v_s - 1.2).abs() < f32::EPSILON);
        assert!((r.v_s_next - 1.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_timestamp_ms_is_positive() {
        let ts = timestamp_ms();
        assert!(ts > 0);
    }
}
