//! Deterministic chain clock.
//!
//! The runtime never reads `SystemTime` or `Date::now`. The clock is
//! seeded at chain construction and only moves forward when an explicit
//! `advance(seconds)` call comes from the test harness or from the
//! playground's `Advance Time` button. This keeps every bytecode execution
//! reproducible across runs and platforms — same input source, same
//! sequence of calls, same `block.timestamp` and same `block.number`.

use serde::{Deserialize, Serialize};

/// Default playground genesis time: 2026-01-01 00:00:00 UTC.
///
/// Chosen to be in the recent past relative to the playground's V0.8 ship
/// window (April 2026) so timestamps shown in the UI feel realistic without
/// drifting as users keep the tab open.
pub const DEFAULT_GENESIS: u64 = 1_767_225_600;

/// Block time assumed when `mine_blocks(n)` is called: 12 seconds, matching
/// post-Merge Ethereum cadence.
pub const DEFAULT_BLOCK_TIME_SECS: u64 = 12;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Clock {
    seconds_since_epoch: u64,
}

impl Clock {
    pub fn new() -> Self {
        Clock {
            seconds_since_epoch: DEFAULT_GENESIS,
        }
    }

    /// Build a clock seeded at an arbitrary unix timestamp.
    pub fn at(seconds_since_epoch: u64) -> Self {
        Clock {
            seconds_since_epoch,
        }
    }

    pub fn timestamp(&self) -> u64 {
        self.seconds_since_epoch
    }

    /// Move the clock forward. `saturating_add` so a malicious or
    /// arithmetic-overflow input stays at `u64::MAX` instead of wrapping
    /// to genesis (which would let a contract observe a `block.timestamp`
    /// going backwards — invariant violation).
    pub fn advance(&mut self, seconds: u64) {
        self.seconds_since_epoch = self.seconds_since_epoch.saturating_add(seconds);
    }

    /// Replace the clock outright. Used by the playground's "Set Time"
    /// debug control and by `restore` after a snapshot.
    pub fn set(&mut self, seconds_since_epoch: u64) {
        self.seconds_since_epoch = seconds_since_epoch;
    }
}

impl Default for Clock {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_seeds_at_genesis() {
        assert_eq!(Clock::new().timestamp(), DEFAULT_GENESIS);
    }

    #[test]
    fn advance_moves_forward() {
        let mut c = Clock::new();
        c.advance(60);
        assert_eq!(c.timestamp(), DEFAULT_GENESIS + 60);
    }

    #[test]
    fn advance_saturates_on_overflow() {
        let mut c = Clock::at(u64::MAX - 5);
        c.advance(100);
        assert_eq!(c.timestamp(), u64::MAX);
    }

    #[test]
    fn json_round_trip() {
        let c = Clock::at(1_700_000_000);
        let json = serde_json::to_string(&c).unwrap();
        let back: Clock = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
    }
}
