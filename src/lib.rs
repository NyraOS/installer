// SPDX-License-Identifier: GPL-3.0-or-later

//! The Nyra OS installer: a full-screen text installer. The logic is in this library so that the
//! tests can drive it without a terminal; `main.rs` only connects it to one.

#![forbid(unsafe_code)]

pub mod app;
pub mod backend;
pub mod harden;
pub mod render;
pub mod rules;
pub mod screens;
pub mod state;
pub mod term;
pub mod texts;
