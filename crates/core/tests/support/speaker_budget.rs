//! One roomy budget for tests of `speaker::cached_line_within`.
//!
//! The product's own budget is 50 ms and expiry is silence, so on a loaded
//! machine a test that asserts a line — or asserts silence for a *reason* —
//! would otherwise be racing two Git spawns. Tests that mean to exercise the
//! 50 ms default call `speaker::cached_line` instead.

use std::time::Duration;

pub const ROOMY: Duration = Duration::from_secs(2);
