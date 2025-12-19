use macroquad::prelude::*;

use wisp::runtime::load_runtime;
use wisp::stdlib::load_stdlib;
use wisp::{eval, parse, Env, Value};

#[cfg(not(target_arch = "wasm32"))]
use std::env as std_env;
#[cfg(not(target_arch = "wasm32"))]
use std::fs;
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
#[cfg(not(target_arch = "wasm32"))]
use rustyline::error::ReadlineError;
#[cfg(not(target_arch = "wasm32"))]
use rustyline::DefaultEditor;
#[cfg(not(target_arch = "wasm32"))]
use wisp::set_script_dir;

fn window_conf() -> Conf {
    // Defaults
    let mut title = "Wisp".to_string();
    let mut width = 800;
    let mut height = 600;

    // Native: Try to read magic comments from script file
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(path) = std_env::args().nth(1)
        && path != "--repl"
            && let Ok(contents) = fs::read_to_string(&path) {
                for line in contents.lines() {
                    let line = line.trim();
                    // Stop at first non-comment, non-empty line
                    if !line.is_empty() && !line.starts_with(';') {
                        break;
                    }
                    // Parse magic comments: ;; @key value
                    if let Some(rest) = line.strip_prefix(";;") {
                        let rest = rest.trim();
                        if let Some(rest) = rest.strip_prefix('@')
                            && let Some((key, value)) = rest.split_once(' ') {
                                let value = value.trim();
                                match key {
                                    "title" => title = value.to_string(),
                                    "width" => {
                                        if let Ok(w) = value.parse() {
                                            width = w;
                                        }
                                    }
                                    "height" => {
                                        if let Ok(h) = value.parse() {
                                            height = h;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                    }
                }
            }

    Conf {
        window_title: title,
        window_width: width,
        window_height: height,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let args: Vec<String> = std_env::args().collect();

        if args.len() > 1 {
            if args[1] == "--repl" {
                repl();
            } else {
                run_game_native(&args[1]).await;
            }
        } else {
            repl();
        }
    }

    #[cfg(target_arch = "wasm32")]
    {
        run_game_wasm().await;
    }
}

/// Extract all (load "...") paths from script source
#[cfg(target_arch = "wasm32")]
fn extract_load_paths(source: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let chars: Vec<char> = source.chars().collect();
    let pattern: Vec<char> = "(load \"".chars().collect();
    let len = chars.len();
    let plen = pattern.len();

    let mut i = 0;
    while i + plen < len {
        if chars[i..i + plen] == pattern[..] {
            let start = i + plen;
            let mut end = start;
            while end < len && chars[end] != '"' {
                end += 1;
            }
            if end < len {
                let path: String = chars[start..end].iter().collect();
                paths.push(path);
            }
            i = end + 1;
        } else {
            i += 1;
        }
    }
    paths
}

/// WASM: Load game script via fetch, preload assets, then run it
#[cfg(target_arch = "wasm32")]
async fn run_game_wasm() {
    use wisp::runtime::{extract_map_paths, extract_sound_paths, preload_map, preload_sound};

    // Try to load game path from config file, fallback to "game.wisp"
    let game_path = load_string("wisp.conf").await
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "game.wisp".to_string());

    // Show loading message
    clear_background(Color::new(0.1, 0.1, 0.2, 1.0));
    draw_text(&format!("Loading {}...", game_path), 300.0, 300.0, 24.0, WHITE);
    next_frame().await;

    // Fetch the script using macroquad's async loader (becomes fetch in WASM)
    let contents = match load_string(&game_path).await {
        Ok(c) => c,
        Err(e) => {
            error_loop(&format!("Failed to load {}: {:?}", game_path, e)).await;
            return;
        }
    };

    // Extract base directory for relative paths
    let base_dir = game_path.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");

    // Preload all scripts referenced by (load "...") - collect all sources
    let mut all_sources = vec![contents.clone()];
    let mut scripts_to_load: Vec<String> = extract_load_paths(&contents);
    let mut loaded_scripts: std::collections::HashSet<String> = std::collections::HashSet::new();

    while let Some(script_path) = scripts_to_load.pop() {
        if loaded_scripts.contains(&script_path) {
            continue;
        }
        loaded_scripts.insert(script_path.clone());

        // Resolve path relative to base_dir
        let full_path = if script_path.starts_with('/') {
            script_path.clone()
        } else if base_dir.is_empty() {
            script_path.clone()
        } else {
            format!("{}/{}", base_dir, script_path)
        };

        clear_background(Color::new(0.1, 0.1, 0.2, 1.0));
        draw_text("Loading scripts...", 300.0, 300.0, 24.0, WHITE);
        draw_text(&format!("Script: {}", script_path), 300.0, 340.0, 16.0, GRAY);
        next_frame().await;

        match load_string(&full_path).await {
            Ok(script_contents) => {
                // Cache the script for later use by (load ...)
                wisp::cache_script(&script_path, script_contents.clone());

                // Check this script for more (load ...) calls
                for nested_path in extract_load_paths(&script_contents) {
                    if !loaded_scripts.contains(&nested_path) {
                        scripts_to_load.push(nested_path);
                    }
                }

                // Add to all_sources for asset extraction
                all_sources.push(script_contents);
            }
            Err(e) => {
                error_loop(&format!("Failed to load script '{}': {:?}", script_path, e)).await;
                return;
            }
        }
    }

    // Extract asset paths from ALL sources (main + loaded scripts)
    let mut map_paths = Vec::new();
    let mut sound_paths = Vec::new();
    for source in &all_sources {
        map_paths.extend(extract_map_paths(source));
        sound_paths.extend(extract_sound_paths(source));
    }

    // Deduplicate
    map_paths.sort();
    map_paths.dedup();
    sound_paths.sort();
    sound_paths.dedup();

    let total_assets = map_paths.len() + sound_paths.len();
    let mut loaded = 0;

    for map_path in &map_paths {
        loaded += 1;
        clear_background(Color::new(0.1, 0.1, 0.2, 1.0));
        draw_text(
            &format!("Loading assets ({}/{})...", loaded, total_assets),
            300.0, 300.0, 24.0, WHITE
        );
        draw_text(&format!("Map: {}", map_path), 300.0, 340.0, 16.0, GRAY);
        next_frame().await;

        if let Err(e) = preload_map(map_path, base_dir).await {
            error_loop(&e).await;
            return;
        }
    }

    for sound_path in &sound_paths {
        loaded += 1;
        clear_background(Color::new(0.1, 0.1, 0.2, 1.0));
        draw_text(
            &format!("Loading assets ({}/{})...", loaded, total_assets),
            300.0, 300.0, 24.0, WHITE
        );
        draw_text(&format!("Sound: {}", sound_path), 300.0, 340.0, 16.0, GRAY);
        next_frame().await;

        if let Err(e) = preload_sound(sound_path, base_dir).await {
            error_loop(&e).await;
            return;
        }
    }

    run_game_from_source(&contents, &game_path).await;
}

/// Show an error message and wait
#[cfg(target_arch = "wasm32")]
async fn error_loop(msg: &str) {
    loop {
        clear_background(Color::new(0.3, 0.1, 0.1, 1.0));
        draw_text("Error:", 50.0, 100.0, 32.0, WHITE);
        draw_text(msg, 50.0, 150.0, 20.0, WHITE);
        draw_text("Check browser console for details", 50.0, 200.0, 16.0, GRAY);
        next_frame().await;
    }
}

/// Native: Load game from filesystem path
#[cfg(not(target_arch = "wasm32"))]
async fn run_game_native(path: &str) {
    // Set script directory for relative path resolution
    set_script_dir(Path::new(path));

    // Load script from filesystem
    let contents = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error reading {}: {}", path, e);
            return;
        }
    };

    run_game_from_source(&contents, path).await;
}

/// Shared: Run game from source code string
async fn run_game_from_source(source: &str, path: &str) {
    let env = Env::new();
    load_stdlib(&env);
    load_runtime(&env);

    // Parse and evaluate the script
    match parse(source) {
        Ok(exprs) => {
            for expr in &exprs {
                if let Err(e) = eval(expr, &env) {
                    show_error(&format!("error: {}", e));
                    return;
                }
            }
        }
        Err(e) => {
            show_error(&format!("parse error in {}: {}", path, e));
            return;
        }
    }

    // Call (init) if defined
    if let Some(init_fn) = env.get("init")
        && let Err(e) = call_fn(&init_fn, vec![]) {
            show_error(&format!("error in init: {}", e));
            return;
        }

    // Game loop
    loop {
        // Call (update) if defined
        if let Some(update_fn) = env.get("update")
            && let Err(e) = call_fn(&update_fn, vec![]) {
                show_error(&format!("error in update: {}", e));
                break;
            }

        // Call (draw) if defined
        if let Some(draw_fn) = env.get("draw")
            && let Err(e) = call_fn(&draw_fn, vec![]) {
                show_error(&format!("error in draw: {}", e));
                break;
            }

        // Check for quit (native only - no escape in browser)
        #[cfg(not(target_arch = "wasm32"))]
        if is_key_pressed(KeyCode::Escape) {
            break;
        }

        next_frame().await;
    }
}

/// Show error - print to stderr on native, log on WASM
fn show_error(msg: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    eprintln!("{}", msg);

    #[cfg(target_arch = "wasm32")]
    macroquad::logging::error!("{}", msg);
}

fn call_fn(func: &Value, args: Vec<Value>) -> Result<Value, String> {
    wisp::eval::apply(func, args)
}

#[cfg(not(target_arch = "wasm32"))]
fn repl() {
    println!("Wisp v0.1.0 (REPL mode - no graphics)");
    println!("Use 'wisp <script.wisp>' to run with graphics");

    let env = Env::new();
    load_stdlib(&env);

    let mut rl = DefaultEditor::new().expect("failed to create editor");

    loop {
        match rl.readline("wisp> ") {
            Ok(line) => {
                if line.trim().is_empty() {
                    continue;
                }
                let _ = rl.add_history_entry(&line);

                match parse(&line) {
                    Ok(exprs) => {
                        for expr in &exprs {
                            match eval(expr, &env) {
                                Ok(result) => {
                                    if !matches!(result, Value::Nil) {
                                        println!("{}", result);
                                    }
                                }
                                Err(e) => eprintln!("error: {}", e),
                            }
                        }
                    }
                    Err(e) => eprintln!("parse error: {}", e),
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("^C");
                break;
            }
            Err(ReadlineError::Eof) => {
                break;
            }
            Err(e) => {
                eprintln!("error: {}", e);
                break;
            }
        }
    }
}
