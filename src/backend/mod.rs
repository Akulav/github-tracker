//! Everything that isn't drawing: GitHub, sign-in, caching, statistics, install and update.

pub mod auth;
pub mod cache;
pub mod demo;
pub mod github;
pub mod install;
pub mod jobs;
pub mod net;
pub mod settings;
pub mod stats;
pub mod update;

pub const APP_NAME: &str = "Akulav Tracker";
pub const APP_ID: &str = "akulav-tracker";
