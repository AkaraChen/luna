pub mod api;
pub mod app;
pub mod db;
pub mod domain;
pub mod entity;
pub mod service;
pub mod web;

pub use app::{rocket, rocket_with_database_url_and_port};
