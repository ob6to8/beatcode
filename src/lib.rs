//! `bc` — an offline, deterministic music compiler and renderer.
//!
//! Implemented per SPEC.md; `goldens/` is the ground truth.

pub mod cli;
pub mod decfmt;
pub mod error;
pub mod events;
pub mod jsonl;
pub mod prng;
pub mod rational;
pub mod render;
pub mod score;
pub mod sha256;
pub mod synth;
pub mod wav;
