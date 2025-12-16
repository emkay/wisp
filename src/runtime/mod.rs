pub mod audio;
pub mod graphics;
pub mod input;
pub mod tiled;

use crate::env::Env;

pub fn load_runtime(env: &Env) {
    audio::load_audio(env);
    graphics::load_graphics(env);
    input::load_input(env);
    tiled::load_tiled(env);
}
