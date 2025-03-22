//! godot plugin for game logic
//!
#![allow(async_fn_in_trait)]

#[cfg(feature = "godot")]
mod scene_manager;
#[cfg(feature = "godot")]
pub use scene_manager::SceneManager;

#[cfg(feature = "ddt")]
mod lark;
#[cfg(feature = "ddt")]
pub use lark::{DataManager, Error as DdtError, SpreadSheet, Table};
/// re-export open_lark and Record
#[cfg(feature = "ddt")]
pub use open_lark::{self, service::bitable::v1::Record};

#[cfg(feature = "gm")]
pub mod gm;
