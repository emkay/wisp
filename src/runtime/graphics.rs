use macroquad::prelude::*;

use crate::env::Env;
use crate::value::{native_fn, Value};

pub fn load_graphics(env: &Env) {
    // Predefined colors
    env.define("color-void", color_to_value(Color::new(0.05, 0.05, 0.1, 1.0)));
    env.define("color-gold", color_to_value(Color::new(1.0, 0.84, 0.0, 1.0)));
    env.define("color-stone", color_to_value(Color::new(0.5, 0.5, 0.5, 1.0)));
    env.define("color-white", color_to_value(Color::new(1.0, 1.0, 1.0, 1.0)));
    env.define("color-black", color_to_value(Color::new(0.0, 0.0, 0.0, 1.0)));

    // Graphics functions
    env.define("rgb", native_fn(rgb));
    env.define("rgba", native_fn(rgba));
    env.define("clear", native_fn(clear_screen));
    env.define("draw-text", native_fn(draw_text_fn));
    env.define("draw-rect", native_fn(draw_rect_fn));
    env.define("screen-width", native_fn(screen_width_fn));
    env.define("screen-height", native_fn(screen_height_fn));

    // Timing
    env.define("dt", native_fn(delta_time));
    env.define("delta-time", native_fn(delta_time));
}

pub fn color_to_value(c: Color) -> Value {
    Value::List(vec![
        Value::Symbol("color".to_string()),
        Value::Float(c.r as f64),
        Value::Float(c.g as f64),
        Value::Float(c.b as f64),
        Value::Float(c.a as f64),
    ])
}

pub fn value_to_color(v: &Value) -> Result<Color, String> {
    match v {
        Value::List(items) if items.len() == 5 => {
            match &items[0] {
                Value::Symbol(s) if s == "color" => {
                    let r = to_f32(&items[1])?;
                    let g = to_f32(&items[2])?;
                    let b = to_f32(&items[3])?;
                    let a = to_f32(&items[4])?;
                    Ok(Color::new(r, g, b, a))
                }
                _ => Err("expected color value (use rgb or rgba to create colors)".to_string()),
            }
        }
        _ => Err(format!("expected color, got {}", v.type_name())),
    }
}

pub fn to_f32(v: &Value) -> Result<f32, String> {
    match v {
        Value::Int(n) => Ok(*n as f32),
        Value::Float(n) => Ok(*n as f32),
        _ => Err(format!("expected number, got {}", v.type_name())),
    }
}

// (rgb r g b) -> color (values 0-255)
fn rgb(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 3 {
        return Err("rgb requires 3 arguments".to_string());
    }
    let r = to_f32(&args[0])? / 255.0;
    let g = to_f32(&args[1])? / 255.0;
    let b = to_f32(&args[2])? / 255.0;
    Ok(color_to_value(Color::new(r, g, b, 1.0)))
}

// (rgba r g b a) -> color (values 0-255)
fn rgba(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 4 {
        return Err("rgba requires 4 arguments".to_string());
    }
    let r = to_f32(&args[0])? / 255.0;
    let g = to_f32(&args[1])? / 255.0;
    let b = to_f32(&args[2])? / 255.0;
    let a = to_f32(&args[3])? / 255.0;
    Ok(color_to_value(Color::new(r, g, b, a)))
}

// (clear color)
fn clear_screen(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("clear requires 1 argument".to_string());
    }
    let color = value_to_color(&args[0])?;
    clear_background(color);
    Ok(Value::Nil)
}

// (draw-text x y text color) or (draw-text x y text color size)
fn draw_text_fn(args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 4 || args.len() > 5 {
        return Err("draw-text requires 4-5 arguments".to_string());
    }
    let x = to_f32(&args[0])?;
    let y = to_f32(&args[1])?;
    let text = match &args[2] {
        Value::String(s) => s.clone(),
        other => format!("{}", other),
    };
    let color = value_to_color(&args[3])?;
    let size = if args.len() == 5 {
        to_f32(&args[4])?
    } else {
        20.0
    };

    draw_text(&text, x, y + size, size, color);
    Ok(Value::Nil)
}

// (draw-rect x y w h color)
fn draw_rect_fn(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 5 {
        return Err("draw-rect requires 5 arguments".to_string());
    }
    let x = to_f32(&args[0])?;
    let y = to_f32(&args[1])?;
    let w = to_f32(&args[2])?;
    let h = to_f32(&args[3])?;
    let color = value_to_color(&args[4])?;

    draw_rectangle(x, y, w, h, color);
    Ok(Value::Nil)
}

// (screen-width) -> int
fn screen_width_fn(args: Vec<Value>) -> Result<Value, String> {
    if !args.is_empty() {
        return Err("screen-width takes no arguments".to_string());
    }
    Ok(Value::Int(screen_width() as i64))
}

// (screen-height) -> int
fn screen_height_fn(args: Vec<Value>) -> Result<Value, String> {
    if !args.is_empty() {
        return Err("screen-height takes no arguments".to_string());
    }
    Ok(Value::Int(screen_height() as i64))
}

// (dt) or (delta-time) -> float (seconds since last frame)
fn delta_time(args: Vec<Value>) -> Result<Value, String> {
    if !args.is_empty() {
        return Err("dt takes no arguments".to_string());
    }
    Ok(Value::Float(get_frame_time() as f64))
}
