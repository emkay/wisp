use std::env as std_env;
use std::fs;
use std::path::Path;

use macroquad::prelude::*;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

use wisp::runtime::load_runtime;
use wisp::stdlib::load_stdlib;
use wisp::{eval, parse, set_script_dir, Env, Value};

fn window_conf() -> Conf {
    Conf {
        window_title: "Wisp".to_string(),
        window_width: 800,
        window_height: 600,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let args: Vec<String> = std_env::args().collect();

    if args.len() > 1 {
        if args[1] == "--repl" {
            repl();
        } else {
            run_game(&args[1]).await;
        }
    } else {
        repl();
    }
}

async fn run_game(path: &str) {
    let env = Env::new();
    load_stdlib(&env);
    load_runtime(&env);

    // Set script directory for relative path resolution in load
    set_script_dir(Path::new(path));

    // Load and evaluate the script
    let contents = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error reading {}: {}", path, e);
            return;
        }
    };

    match parse(&contents) {
        Ok(exprs) => {
            for expr in &exprs {
                if let Err(e) = eval(expr, &env) {
                    eprintln!("error: {}", e);
                    return;
                }
            }
        }
        Err(e) => {
            eprintln!("parse error: {}", e);
            return;
        }
    }

    // Call (init) if defined
    if let Some(init_fn) = env.get("init")
        && let Err(e) = call_fn(&init_fn, vec![]) {
            eprintln!("error in init: {}", e);
            return;
        }

    // Game loop
    loop {
        // Call (update) if defined
        if let Some(update_fn) = env.get("update")
            && let Err(e) = call_fn(&update_fn, vec![]) {
                eprintln!("error in update: {}", e);
                break;
            }

        // Call (draw) if defined
        if let Some(draw_fn) = env.get("draw")
            && let Err(e) = call_fn(&draw_fn, vec![]) {
                eprintln!("error in draw: {}", e);
                break;
            }

        // Check for quit
        if is_key_pressed(KeyCode::Escape) {
            break;
        }

        next_frame().await;
    }
}

fn call_fn(func: &Value, args: Vec<Value>) -> Result<Value, String> {
    wisp::eval::apply(func, args)
}

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
