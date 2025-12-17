use macroquad::prelude::*;

use crate::env::Env;
use crate::value::{native_fn, Value};

pub fn load_input(env: &Env) {
    env.define("key-pressed?", native_fn(key_pressed));
    env.define("key-down?", native_fn(key_down));
    env.define("key-released?", native_fn(key_released));
}

fn symbol_to_keycode(s: &str) -> Option<KeyCode> {
    match s {
        // Arrow keys
        "left" => Some(KeyCode::Left),
        "right" => Some(KeyCode::Right),
        "up" => Some(KeyCode::Up),
        "down" => Some(KeyCode::Down),

        // Common keys
        "space" => Some(KeyCode::Space),
        "enter" | "return" => Some(KeyCode::Enter),
        "escape" | "esc" => Some(KeyCode::Escape),
        "tab" => Some(KeyCode::Tab),
        "backspace" => Some(KeyCode::Backspace),

        // Letters
        "a" => Some(KeyCode::A),
        "b" => Some(KeyCode::B),
        "c" => Some(KeyCode::C),
        "d" => Some(KeyCode::D),
        "e" => Some(KeyCode::E),
        "f" => Some(KeyCode::F),
        "g" => Some(KeyCode::G),
        "h" => Some(KeyCode::H),
        "i" => Some(KeyCode::I),
        "j" => Some(KeyCode::J),
        "k" => Some(KeyCode::K),
        "l" => Some(KeyCode::L),
        "m" => Some(KeyCode::M),
        "n" => Some(KeyCode::N),
        "o" => Some(KeyCode::O),
        "p" => Some(KeyCode::P),
        "q" => Some(KeyCode::Q),
        "r" => Some(KeyCode::R),
        "s" => Some(KeyCode::S),
        "t" => Some(KeyCode::T),
        "u" => Some(KeyCode::U),
        "v" => Some(KeyCode::V),
        "w" => Some(KeyCode::W),
        "x" => Some(KeyCode::X),
        "y" => Some(KeyCode::Y),
        "z" => Some(KeyCode::Z),

        // Numbers
        "0" => Some(KeyCode::Key0),
        "1" => Some(KeyCode::Key1),
        "2" => Some(KeyCode::Key2),
        "3" => Some(KeyCode::Key3),
        "4" => Some(KeyCode::Key4),
        "5" => Some(KeyCode::Key5),
        "6" => Some(KeyCode::Key6),
        "7" => Some(KeyCode::Key7),
        "8" => Some(KeyCode::Key8),
        "9" => Some(KeyCode::Key9),

        _ => None,
    }
}

fn get_keycode(v: &Value) -> Result<KeyCode, String> {
    match v {
        Value::Symbol(s) => symbol_to_keycode(s)
            .ok_or_else(|| format!("unknown key: {}", s)),
        Value::String(s) => symbol_to_keycode(s)
            .ok_or_else(|| format!("unknown key: {}", s)),
        _ => Err(format!("expected key symbol, got {}", v.type_name())),
    }
}

fn key_check(args: Vec<Value>, check: fn(KeyCode) -> bool, ctx: &str) -> Result<Value, String> {
    if args.len() != 1 {
        return Err(format!("{} requires 1 argument", ctx));
    }
    let key = get_keycode(&args[0])?;
    Ok(Value::Bool(check(key)))
}

fn key_pressed(args: Vec<Value>) -> Result<Value, String> {
    key_check(args, is_key_pressed, "key-pressed?")
}

fn key_down(args: Vec<Value>) -> Result<Value, String> {
    key_check(args, is_key_down, "key-down?")
}

fn key_released(args: Vec<Value>) -> Result<Value, String> {
    key_check(args, is_key_released, "key-released?")
}
