//! Monadeck core — the small, framework-agnostic orchestration layer.
//!
//! Everything Monadeck actually does reduces to a handful of jobs, each ported
//! from Envision's proven implementation and kept free of any UI dependency so
//! it can be unit-tested and driven from the Tauri layer (or a CLI, or tests):
//!
//! - [`cmd_runner`] — spawn `monado-service`, stream its stdout/stderr to logs.
//! - [`active_runtime`] — point the OpenXR loader at monado, with backup/restore.
//! - [`openvr_paths`] — register xrizer as the OpenVR runtime, with backup/restore.
//! - [`setcap`] — set/verify `CAP_SYS_NICE=eip` on the service binary.
//! - [`preflight`] — runtime prerequisite checks (udev rules, pkexec) for other boxes.
//! - [`devices`] — live device list via libmonado (`auto_connect`).
//! - [`plugins`] — launch arbitrary apps by explicit path alongside the service.
//! - [`config`] / [`paths`] — persisted settings and well-known file locations.

pub mod active_runtime;
pub mod cmd_runner;
pub mod collections;
pub mod config;
pub mod desktop;
pub mod devices;
pub mod favorites;
pub mod gpu;
pub mod installer;
pub mod launch_options;
pub mod monado_conn;
pub mod openvr_paths;
pub mod overlay_config;
pub mod paths;
pub mod playspace_overrides;
pub mod playtime;
pub mod plugins;
pub mod preflight;
pub mod proton;
pub mod setcap;
pub mod steam;
pub mod uevr;

pub use config::MonadeckConfig;
