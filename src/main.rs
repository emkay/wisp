use std::env as std_env;
use std::fs;

use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

use wisp::{eval, parse, Env};
use wisp::stdlib::load_stdlib;

fn main() {
    let args: Vec<String> = std_env::args().collect();

    if args.len() > 1 {
        run_file(&args[1]);
    } else {
        repl();
    }
}

fn run_file(path: &str) {
    let env = Env::new();
    load_stdlib(&env);

    let contents = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error reading {}: {}", path, e);
            std::process::exit(1);
        }
    };

    match parse(&contents) {
        Ok(exprs) => {
            for expr in &exprs {
                match eval(expr, &env) {
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("error: {}", e);
                        std::process::exit(1);
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("parse error: {}", e);
            std::process::exit(1);
        }
    }
}

fn repl() {
    println!("Wisp v0.1.0");

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
                                    if !matches!(result, wisp::Value::Nil) {
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
