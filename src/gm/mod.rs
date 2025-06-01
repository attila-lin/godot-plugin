//! GM Tool Support
//! 主要工作：在游戏中创建一个 HTTP 服务器，然后进行接受指令请求
//! 用户只用实现相关的指令处理逻辑
//!
//! 会自动注册一个界面在场景中，然后可以点击

#[cfg(feature = "godot")]
pub mod ui;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Command {
    pub name: String,
    pub command: String,
    pub args: Vec<Arg>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Arg {
    pub name: String,
    pub r#type: ArgType,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub enum ArgType {
    Int,
    Number,
    String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RunCommandRequest {
    pub command: String,
    #[serde(default)]
    pub args: Vec<Value>,
}
