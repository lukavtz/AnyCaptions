//! AnyCaptions core: domain, preferences, audio, transcription, translation.
//!
//! This crate contains everything that does not depend on a GUI framework.
//! GUI-specific code lives in adapter crates.

pub mod audio;
pub mod caption;
pub mod dictation;
pub mod domain;
pub mod ipc;
pub mod models;
pub mod native;
pub mod pipeline;
pub mod preferences;
pub mod presentation;
pub mod transcription;
pub mod translation;
pub mod update;
pub mod updater;
pub mod window_geometry;
