//! godot plugin for game logic

#[cfg(feature = "godot")]
mod scene_manager;
#[cfg(feature = "godot")]
pub use scene_manager::SceneManager;

#[cfg(feature = "ddt")]
mod lark;
#[cfg(feature = "ddt")]
pub use lark::{DateManager, Table};
