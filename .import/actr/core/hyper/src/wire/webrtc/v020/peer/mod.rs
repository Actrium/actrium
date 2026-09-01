//! Per-peer RTC control plane.
//!
//! One `RtcPeerController` per remote `ActrId`, each the single writer of its
//! peer's control state. The reducer decides all transitions; the effect shell
//! runs blocking WebRTC operations on effect tasks; the handle is the read/send
//! face for the manager, gate, and business.

pub(crate) mod controller;
pub(crate) mod effect_port;
pub(crate) mod effects;
pub(crate) mod handle;
pub(crate) mod lane;
pub(crate) mod lane_handle;
pub(crate) mod policy_clock;
pub(crate) mod reducer;
pub(crate) mod send;
pub(crate) mod session;
pub(crate) mod state;

#[cfg(test)]
#[path = "reducer_tests.rs"]
mod reducer_tests;

#[cfg(test)]
#[path = "controller_tests.rs"]
mod controller_tests;

#[cfg(test)]
#[path = "t1_controller_tests.rs"]
mod t1_controller_tests;

#[cfg(test)]
#[path = "send_tests.rs"]
mod send_tests;
