pub mod audio;
pub mod graphics;
pub mod input;
pub mod tiled;

use crate::env::Env;

// Re-export preloading functions
pub use audio::{extract_sound_paths, preload_sound};
pub use tiled::{extract_map_paths, preload_map};

pub fn load_runtime(env: &Env) {
    audio::load_audio(env);
    graphics::load_graphics(env);
    input::load_input(env);
    tiled::load_tiled(env);
}
