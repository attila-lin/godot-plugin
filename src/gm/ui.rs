use godot::classes::Button;
use godot::classes::Control;
use godot::classes::EditorPlugin;
use godot::classes::HBoxContainer;
use godot::classes::IControl;
use godot::classes::IEditorPlugin;
use godot::classes::IPanelContainer;
use godot::classes::ItemList;
use godot::classes::Label;
use godot::classes::PanelContainer;
use godot::classes::TextEdit;
use godot::classes::VBoxContainer;
use godot::prelude::*;
use serde_json::Number;
use serde_json::Value;

use super::Arg;
use super::ArgType;
use super::Command;
use super::RunCommandRequest;

/// read <https://godot-rust.github.io/book/recipes/editor-plugin/index.html?highlight=%5Bclass(tool)%5D#creating-an-editorplugin>
#[derive(GodotClass)]
#[class(tool, init, base=EditorPlugin)]
pub struct GMCommandTool {
    commands: Vec<Command>,

    detail: Option<Gd<GMPanelContainer>>,

    base: Base<EditorPlugin>,
}

const URI: &str = "/gm/commands";
const DEFAULT_GM_TOOL_BASE_URL: &str = "http://10.242.104.45:3000";

#[godot_api]
impl IEditorPlugin for GMCommandTool {
    fn enter_tree(&mut self) {
        // read all gm command from server
        match fetch_commands() {
            Ok(commands) => self.commands = commands,
            Err(err) => {
                self.commands.clear();
                godot_error!("failed to fetch GM commands: {err}");
            }
        }

        godot_print!("1: {:?}", self.commands);

        let mut b_container = HBoxContainer::new_alloc();

        // left
        let mut command_list = ItemList::new_alloc();

        let callable = self.base().callable("item_selected");
        command_list.connect("item_selected", &callable);
        // {
        //     let mut bind = self.base_mut();

        //     bind.add_child(&command_list);
        // }
        command_list.set_name("GMCommandList");
        command_list.set_custom_minimum_size(Vector2 { x: 200., y: 300. });
        // command_list.set_item_selectable(idx, selectable);

        {
            for c in &self.commands {
                command_list.add_item(&c.name);
            }
        }

        b_container.add_child(&command_list);

        let panel = GMPanelContainer::new_alloc();
        b_container.add_child(&panel);

        self.detail = Some(panel);

        self.base_mut()
            .add_control_to_bottom_panel(&b_container, "GM Tool");
    }

    fn exit_tree(&mut self) {}

    fn get_plugin_name(&self) -> GString {
        "GMTool".into()
    }
}

#[godot_api]
impl GMCommandTool {
    #[func]
    fn item_selected(&mut self, index: u32) {
        godot_print!("{index}");

        // let ei = self.base_mut().get_editor_interface().unwrap();
        // let mut bc = ei.get_base_control().unwrap();

        // let mut panel = GMPanelContainer::new_alloc();
        // bc.add_child(&panel);
        let Some(command) = self.commands.get(index as usize).cloned() else {
            godot_error!("selected GM command index out of range: {index}");
            return;
        };
        let Some(detail) = self.detail.as_ref() else {
            godot_error!("GM detail panel is not initialized");
            return;
        };
        let mut panel = detail.clone();
        let mut bind_mut = panel.bind_mut();
        bind_mut.update_detail(command);

        // let mut window = panel.get_window().unwrap();
        // window.popup_centered();
    }
}

#[derive(GodotClass)]
#[class(tool, base = PanelContainer)]
pub struct GMPanelContainer {
    h_box: Option<Gd<HBoxContainer>>,
    /// 放 args 的
    v_box: Option<Gd<VBoxContainer>>,

    /// 中文
    label: Option<Gd<Label>>,

    /// command
    command_label: Option<Gd<Label>>,

    args: Vec<Gd<ArgInput>>,

    base: Base<PanelContainer>,
}

#[godot_api]
impl IPanelContainer for GMPanelContainer {
    fn init(base: Base<PanelContainer>) -> Self {
        // let label = Label::new_alloc();

        Self {
            h_box: None,
            v_box: None,
            label: None,
            command_label: None,
            args: Vec::new(),
            base,
        }
    }

    fn ready(&mut self) {
        let mut h_box = HBoxContainer::new_alloc();
        self.base_mut().add_child(&h_box);

        let label = Label::new_alloc();
        // let label = self.label.unwrap()
        h_box.add_child(&label);

        let command_label = Label::new_alloc();
        h_box.add_child(&command_label);

        let mut v_box = VBoxContainer::new_alloc();
        h_box.add_child(&v_box);
        v_box.set_custom_minimum_size(Vector2 { x: 300., y: 400. });

        let mut btn = Button::new_alloc();
        h_box.add_child(&btn);

        btn.set_text("Run");
        let callable = self.base().callable("_on_run_pressed");
        btn.connect("pressed", &callable);

        // h_box.set_name("hbox");
        self.h_box = Some(h_box);
        self.v_box = Some(v_box);

        self.label = Some(label);
        self.command_label = Some(command_label);
    }
}

#[godot_api]
impl GMPanelContainer {
    fn update_detail(&mut self, data: Command) {
        let mut label = self.label.as_ref().unwrap().clone();
        label.set_text(&data.name);

        let mut command_label = self.command_label.as_ref().unwrap().clone();
        command_label.set_text(&data.command);

        let mut v_box = self.v_box.as_ref().unwrap().clone();
        {
            let children = v_box.get_children();
            for mut child in children.iter_shared() {
                v_box.remove_child(&child);
                child.queue_free();
            }
        }
        let mut arg_inputs = vec![];
        for arg in &data.args {
            let mut arg_input = ArgInput::new_alloc();
            v_box.add_child(&arg_input);
            {
                let mut bind = arg_input.bind_mut();
                bind.set_arg(arg);
            }

            arg_inputs.push(arg_input);
        }
        self.args = arg_inputs;
    }

    #[func]
    fn _on_run_pressed(&mut self) {
        godot_print!("run");

        let Some(name) = self.command_label.as_ref().cloned() else {
            godot_error!("GM command label is not initialized");
            return;
        };

        let mut args = vec![];
        for input in &self.args {
            let bind = input.bind();
            match bind.get_value() {
                Ok(value) => args.push(value),
                Err(err) => {
                    godot_error!("invalid GM argument: {err}");
                    return;
                }
            }
        }

        let req = RunCommandRequest {
            command: name.get_text().into(),
            args,
        };

        let resp = reqwest::blocking::Client::new()
            .post(format!("{}/gm/run", gm_tool_base_url()))
            .json(&req)
            .send();

        match resp {
            Ok(resp) => godot_print!("{:?}", resp.status()),
            Err(err) => godot_error!("failed to run GM command: {err}"),
        }
    }
}

// impl GMPanelContainer {
//     fn set_data(&mut self, data: Command) {
//         let label = self.label.as_deref_mut().unwrap();
//         label.set_text(&data.name);
//     }
// }

#[derive(GodotClass)]
#[class(tool, base = Control)]
pub struct ArgInput {
    text_input: Option<Gd<TextEdit>>,
    r#type: Option<ArgType>,

    base: Base<Control>,
}

#[godot_api]
impl IControl for ArgInput {
    fn init(base: Base<Control>) -> Self {
        Self {
            text_input: None,
            r#type: None,
            base,
        }
    }

    fn ready(&mut self) {
        self.base_mut()
            .set_custom_minimum_size(Vector2 { x: 100., y: 400. });
    }
}

#[godot_api]
impl ArgInput {
    fn set_arg(&mut self, arg: &Arg) {
        let mut label = Label::new_alloc();
        self.base_mut().add_child(&label);
        label.set_text(&arg.name);

        match arg.r#type {
            ArgType::Int | ArgType::Number => {
                let mut text_input = TextEdit::new_alloc();
                self.base_mut().add_child(&text_input);
                text_input.set_custom_minimum_size(Vector2 { x: 300., y: 100. });
                text_input.set_position(Vector2 { x: 0., y: 100. });
                text_input.set_size(Vector2 { x: 300., y: 100. });
                self.text_input = Some(text_input);
            }
            ArgType::String => {
                let mut text_input = TextEdit::new_alloc();
                self.base_mut().add_child(&text_input);
                text_input.set_custom_minimum_size(Vector2 { x: 300., y: 100. });
                text_input.set_position(Vector2 { x: 0., y: 100. });
                text_input.set_size(Vector2 { x: 300., y: 100. });
                self.text_input = Some(text_input);
            }
        }

        self.r#type = Some(arg.r#type);
    }

    fn get_value(&self) -> Result<Value, String> {
        let input = self
            .text_input
            .as_ref()
            .ok_or_else(|| "argument input is not initialized".to_owned())?
            .clone();
        let text = input.get_text().to_string();

        match self.r#type {
            Some(ArgType::Int) => {
                let value = text
                    .parse::<i64>()
                    .map_err(|err| format!("expected integer, got `{text}`: {err}"))?;
                Ok(Value::Number(Number::from(value)))
            }
            Some(ArgType::Number) => {
                let value = text
                    .parse::<f64>()
                    .map_err(|err| format!("expected number, got `{text}`: {err}"))?;
                let value = Number::from_f64(value)
                    .ok_or_else(|| format!("number must be finite, got `{text}`"))?;
                Ok(Value::Number(value))
            }
            Some(ArgType::String) => Ok(Value::String(text)),
            None => Err("argument type is not initialized".to_owned()),
        }
    }
}

fn gm_tool_base_url() -> String {
    std::env::var("GM_TOOL_BASE_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_GM_TOOL_BASE_URL.to_owned())
        .trim_end_matches('/')
        .to_owned()
}

fn fetch_commands() -> Result<Vec<Command>, reqwest::Error> {
    reqwest::blocking::get(format!("{}{}", gm_tool_base_url(), URI))?.json()
}
