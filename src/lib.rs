//! feishu-export — CLI 入口
//!
//! This binary is a thin wrapper around `feishu-core`.
//! All heavy logic lives in the `feishu-core` crate.

pub mod cmd;

pub use cmd::*;
