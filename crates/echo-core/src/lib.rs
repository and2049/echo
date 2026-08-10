//! Frontend-agnostic core of echo: Spotify + local-library worker, domain models, config,
//! app state and the events that tie a frontend to the worker.
//!
//! Frontends (the ratatui `spotify` binary, the GPUI `echo` desktop app) depend on this crate,
//! spawn [`worker::Worker`] on a tokio runtime and talk to it over the two event channels in
//! [`events`]. Nothing in here may depend on a rendering or input library.

pub mod action_menu;
pub mod app;
pub mod apply_worker_event;
pub mod artwork;
pub mod bootstrap;
pub mod commands;
pub mod config;
pub mod events;
pub mod i18n;
pub mod image_tasks;
pub mod intent;
pub mod models;
pub mod platform;
pub mod theme;
pub mod thumbnails;
pub mod update;
pub mod worker;
