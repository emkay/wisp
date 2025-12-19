//! Wisp a Scheme / Lisp like programming language for writing games.
//!
//! It includes the [Macroquad](https://macroquad.rs/) game library and exposes an interface to it
//! allowing you access to pretty much all the features it offers. It also knows how to read
//! [Tiled maps](https://mapeditor.org), allowing you to build out maps and add metadata to
//! objects.
//!
pub mod env;
pub mod eval;
pub mod parse;
pub mod runtime;
pub mod stdlib;
pub mod value;

pub use env::Env;
pub use eval::{cache_script, eval, resolve_path, set_script_dir};
pub use parse::parse;
pub use value::Value;
