//! 场景管理，用于管理场景的切换和更新
use std::sync::{LazyLock, Mutex};

use godot::prelude::*;

#[derive(GodotClass)]
#[class(no_init)]
pub struct SceneManager {
    /// Stack to keep track of scenes
    scene_stack: Mutex<Vec<String>>,
}

#[godot_api]
impl SceneManager {
    fn init() -> Self {
        SceneManager {
            scene_stack: Mutex::new(Vec::new()),
        }
    }

    /// Get the singleton instance of the SceneManager
    pub fn singleton() -> &'static Self {
        static SINGLETON: LazyLock<SceneManager> = LazyLock::new(SceneManager::init);

        &SINGLETON
    }

    /// Load a new scene
    pub fn load_scene(&self, scene_tree: &mut Gd<SceneTree>, scene_path: &str) {
        // Push the current scene to the stack
        if let Some(current_scene) = self.get_current_scene(scene_tree) {
            let mut scene_stack = self.scene_stack.lock().unwrap();
            scene_stack.push(current_scene);
        }

        // Load the new scene
        scene_tree.change_scene_to_file(scene_path);
    }

    /// Go back to the previous scene
    pub fn go_back(&self, scene_tree: &mut Gd<SceneTree>) {
        let mut scene_stack = self.scene_stack.lock().unwrap();

        // Pop the current scene
        if let Some(previous_scene) = scene_stack.pop() {
            // Load the previous scene
            scene_tree.change_scene_to_file(&previous_scene);
        }
    }

    fn get_current_scene(&self, scene_tree: &mut Gd<SceneTree>) -> Option<String> {
        let current_scene = scene_tree.get_current_scene();
        current_scene.map(|node| node.get_scene_file_path().to_string())
    }
}
