//! Audio module - supports both native and WASM with preloading

use std::cell::RefCell;
use std::collections::HashMap;

use macroquad::audio::{play_sound, stop_sound, set_sound_volume, Sound, PlaySoundParams};

use crate::env::Env;
use crate::value::{native_fn, Value};

#[cfg(not(target_arch = "wasm32"))]
use std::fs;
#[cfg(not(target_arch = "wasm32"))]
use macroquad::audio::load_sound_from_bytes;
#[cfg(not(target_arch = "wasm32"))]
use crate::eval::resolve_path;

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

// --- Native: synchronous loading ---

#[cfg(not(target_arch = "wasm32"))]
fn load_sound_sync(path: &str) -> Result<Sound, String> {
    let bytes = fs::read(path).map_err(|e| format!("Failed to read audio '{}': {}", path, e))?;
    let sound = futures::executor::block_on(load_sound_from_bytes(&bytes))
        .map_err(|e| format!("Failed to load audio '{}': {}", path, e))?;
    Ok(sound)
}

#[cfg(not(target_arch = "wasm32"))]
fn load_sound_fn(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("load-sound: requires 1 argument".to_string());
    }

    let path_arg = args[0].as_string("load-sound")?;
    let resolved = resolve_path(&path_arg)
        .map_err(|e| format!("load-sound: {}", e))?;
    let resolved_str = resolved.to_string_lossy().to_string();

    let sound = load_sound_sync(&resolved_str)?;

    SOUNDS.with(|sounds| {
        sounds.borrow_mut().insert(resolved_str.clone(), sound);
    });

    Ok(Value::String(resolved_str))
}

// --- WASM: preloaded sounds ---

#[cfg(target_arch = "wasm32")]
fn load_sound_fn(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("load-sound: requires 1 argument".to_string());
    }

    let path = args[0].as_string("load-sound")?;

    // Check if sound was preloaded
    if is_sound_loaded(&path) {
        Ok(Value::String(path))
    } else {
        Err(format!(
            "load-sound: '{}' was not preloaded. Ensure the sound is referenced in your script.",
            path
        ))
    }
}

// --- Shared playback functions ---

fn play_sound_impl(args: Vec<Value>, looped: bool, ctx: &str) -> Result<Value, String> {
    if args.len() != 1 {
        return Err(format!("{}: requires 1 argument", ctx));
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

fn play_sound_fn(args: Vec<Value>) -> Result<Value, String> {
    play_sound_impl(args, false, "play-sound")
}

fn play_music_fn(args: Vec<Value>) -> Result<Value, String> {
    play_sound_impl(args, true, "play-music")
}

fn stop_sound_fn(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("stop-sound: requires 1 argument".to_string());
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

fn set_volume_fn(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("set-volume: requires 2 arguments".to_string());
    }

    let sound_id = args[0].as_string("set-volume")?;
    let volume = args[1].as_f32("set-volume")?;

    if !(0.0..=1.0).contains(&volume) {
        return Err(format!(
            "set-volume: volume must be between 0.0 and 1.0, got {}",
            volume
        ));
    }

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

// --- Preloading (for WASM) ---

/// Extract sound paths from script source (finds all load-sound calls)
pub fn extract_sound_paths(source: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut chars = source.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '(' {
            let rest: String = chars.clone().take(12).collect();
            if rest.starts_with("load-sound ") || rest.starts_with("load-sound\"") {
                // Skip "load-sound"
                for _ in 0..10 { chars.next(); }
                // Skip whitespace
                while chars.peek().map(|c| c.is_whitespace()).unwrap_or(false) {
                    chars.next();
                }
                // Expect quote
                if chars.peek() == Some(&'"') {
                    chars.next();
                    let path: String = chars.by_ref().take_while(|c| *c != '"').collect();
                    if !path.is_empty() {
                        paths.push(path);
                    }
                }
            }
        }
    }

    paths
}

/// Preload a sound asynchronously (for WASM)
pub async fn preload_sound(path: &str, base_dir: &str) -> Result<String, String> {
    use macroquad::audio::load_sound;

    // Resolve path relative to base directory
    let full_path = if path.starts_with('/') || path.starts_with("http") || base_dir.is_empty() {
        path.to_string()
    } else {
        format!("{}/{}", base_dir.trim_end_matches('/'), path)
    };

    // Load sound async
    let sound = load_sound(&full_path).await
        .map_err(|e| format!("Failed to load sound '{}': {:?}", full_path, e))?;

    // Store with original path as key
    SOUNDS.with(|sounds| {
        sounds.borrow_mut().insert(path.to_string(), sound);
    });

    Ok(path.to_string())
}

/// Check if a sound is already loaded
pub fn is_sound_loaded(path: &str) -> bool {
    SOUNDS.with(|sounds| sounds.borrow().contains_key(path))
}
