use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;

use macroquad::audio::{load_sound_from_bytes, play_sound, stop_sound, set_sound_volume, Sound, PlaySoundParams};

use crate::env::Env;
use crate::eval::resolve_path;
use crate::value::{native_fn, Value};

thread_local! {
    static SOUNDS: RefCell<HashMap<String, Sound>> = RefCell::new(HashMap::new());
}

pub fn load_audio(env: &Env) {
    env.define("load-sound", native_fn(load_sound_fn));
    env.define("play-sound", native_fn(play_sound_fn));
    env.define("play-music", native_fn(play_music_fn));
    env.define("stop-sound", native_fn(stop_sound_fn));
    env.define("set-volume", native_fn(set_volume_fn));
}

fn load_sound_sync(path: &str) -> Result<Sound, String> {
    let bytes = fs::read(path).map_err(|e| format!("Failed to read audio '{}': {}", path, e))?;
    let sound = futures::executor::block_on(load_sound_from_bytes(&bytes))
        .map_err(|e| format!("Failed to load audio '{}': {}", path, e))?;
    Ok(sound)
}

// (load-sound "path.ogg") -> sound-id
fn load_sound_fn(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("load-sound requires 1 argument".to_string());
    }

    let path_arg = args[0].as_string("load-sound")?;
    let resolved = resolve_path(&path_arg);
    let resolved_str = resolved.to_string_lossy().to_string();

    let sound = load_sound_sync(&resolved_str)?;

    SOUNDS.with(|sounds| {
        sounds.borrow_mut().insert(resolved_str.clone(), sound);
    });

    Ok(Value::String(resolved_str))
}

fn play_sound_impl(args: Vec<Value>, looped: bool, ctx: &str) -> Result<Value, String> {
    if args.len() != 1 {
        return Err(format!("{} requires 1 argument", ctx));
    }

    let sound_id = args[0].as_string(ctx)?;

    SOUNDS.with(|sounds| {
        let sounds = sounds.borrow();
        if let Some(sound) = sounds.get(&sound_id) {
            play_sound(sound, PlaySoundParams { looped, volume: 1.0 });
            Ok(Value::Nil)
        } else {
            Err(format!("{}: unknown sound '{}'", ctx, sound_id))
        }
    })
}

// (play-sound sound-id) - play sound once
fn play_sound_fn(args: Vec<Value>) -> Result<Value, String> {
    play_sound_impl(args, false, "play-sound")
}

// (play-music sound-id) - play sound looped (for background music)
fn play_music_fn(args: Vec<Value>) -> Result<Value, String> {
    play_sound_impl(args, true, "play-music")
}

// (stop-sound sound-id) - stop a playing sound
fn stop_sound_fn(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("stop-sound requires 1 argument".to_string());
    }

    let sound_id = args[0].as_string("stop-sound")?;

    SOUNDS.with(|sounds| {
        let sounds = sounds.borrow();
        if let Some(sound) = sounds.get(&sound_id) {
            stop_sound(sound);
            Ok(Value::Nil)
        } else {
            Err(format!("stop-sound: unknown sound '{}'", sound_id))
        }
    })
}

// (set-volume sound-id volume) - set volume (0.0 to 1.0)
fn set_volume_fn(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("set-volume requires 2 arguments".to_string());
    }

    let sound_id = args[0].as_string("set-volume")?;
    let volume = args[1].as_f32("set-volume")?;

    SOUNDS.with(|sounds| {
        let sounds = sounds.borrow();
        if let Some(sound) = sounds.get(&sound_id) {
            set_sound_volume(sound, volume);
            Ok(Value::Nil)
        } else {
            Err(format!("set-volume: unknown sound '{}'", sound_id))
        }
    })
}
