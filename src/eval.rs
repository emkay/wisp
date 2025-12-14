use std::cell::RefCell;
use std::fs;
use std::path::Path;

use crate::env::Env;
use crate::parse::parse;
use crate::value::Value;

thread_local! {
    static TRACE_ENABLED: RefCell<bool> = const { RefCell::new(false) };
    static TRACE_DEPTH: RefCell<usize> = const { RefCell::new(0) };
}

pub fn eval(expr: &Value, env: &Env) -> Result<Value, String> {
    match expr {
        Value::Nil | Value::Bool(_) | Value::Int(_) | Value::Float(_) | Value::String(_) => {
            Ok(expr.clone())
        }

        Value::Symbol(name) => env
            .get(name)
            .ok_or_else(|| format!("undefined variable: {}", name)),

        Value::List(items) if items.is_empty() => Ok(Value::Nil),

        Value::List(items) => {
            let first = &items[0];

            // Check for special forms
            if let Value::Symbol(name) = first {
                match name.as_str() {
                    "quote" => return eval_quote(items),
                    "if" => return eval_if(items, env),
                    "cond" => return eval_cond(items, env),
                    "define" => return eval_define(items, env),
                    "set!" => return eval_set(items, env),
                    "let" => return eval_let(items, env),
                    "fn" | "lambda" => return eval_fn(items, env),
                    "do" | "begin" => return eval_do(items, env),
                    "and" => return eval_and(items, env),
                    "or" => return eval_or(items, env),
                    "load" => return eval_load(items, env),
                    "trace-on" => return eval_trace_on(),
                    "trace-off" => return eval_trace_off(),
                    _ => {}
                }
            }

            // Function call
            let func = eval(first, env)?;
            let args: Result<Vec<Value>, String> =
                items[1..].iter().map(|arg| eval(arg, env)).collect();
            let args = args?;

            trace_enter(expr);
            let result = apply(&func, args);
            trace_exit(&result);
            result
        }

        _ => Ok(expr.clone()),
    }
}

pub fn apply(func: &Value, args: Vec<Value>) -> Result<Value, String> {
    match func {
        Value::NativeFn(f) => f(args),
        Value::Fn { params, body, env } => {
            if args.len() != params.len() {
                return Err(format!(
                    "expected {} arguments, got {}",
                    params.len(),
                    args.len()
                ));
            }
            let local_env = Env::with_parent(env);
            for (param, arg) in params.iter().zip(args.into_iter()) {
                local_env.define(param, arg);
            }
            eval(body, &local_env)
        }
        _ => Err(format!("not a function: {}", func)),
    }
}

fn eval_quote(items: &[Value]) -> Result<Value, String> {
    if items.len() != 2 {
        return Err("quote requires exactly 1 argument".to_string());
    }
    Ok(items[1].clone())
}

fn eval_if(items: &[Value], env: &Env) -> Result<Value, String> {
    if items.len() < 3 || items.len() > 4 {
        return Err("if requires 2 or 3 arguments".to_string());
    }
    let cond = eval(&items[1], env)?;
    if cond.is_truthy() {
        eval(&items[2], env)
    } else if items.len() == 4 {
        eval(&items[3], env)
    } else {
        Ok(Value::Nil)
    }
}

fn eval_cond(items: &[Value], env: &Env) -> Result<Value, String> {
    for clause in &items[1..] {
        if let Value::List(parts) = clause {
            if parts.is_empty() {
                return Err("cond clause cannot be empty".to_string());
            }

            let test = if let Value::Symbol(s) = &parts[0] {
                s == "else"
            } else {
                false
            };

            if test || eval(&parts[0], env)?.is_truthy() {
                let mut result = Value::Nil;
                for expr in &parts[1..] {
                    result = eval(expr, env)?;
                }
                return Ok(result);
            }
        } else {
            return Err("cond clause must be a list".to_string());
        }
    }
    Ok(Value::Nil)
}

fn eval_define(items: &[Value], env: &Env) -> Result<Value, String> {
    if items.len() < 2 {
        return Err("define requires at least 1 argument".to_string());
    }

    match &items[1] {
        // (define x 10)
        Value::Symbol(name) => {
            if items.len() != 3 {
                return Err("define requires exactly 2 arguments".to_string());
            }
            let value = eval(&items[2], env)?;
            env.define(name, value);
            Ok(Value::Nil)
        }
        // (define (f x y) body)
        Value::List(sig) if !sig.is_empty() => {
            if let Value::Symbol(name) = &sig[0] {
                let params: Result<Vec<String>, String> = sig[1..]
                    .iter()
                    .map(|p| match p {
                        Value::Symbol(s) => Ok(s.clone()),
                        _ => Err("parameter must be a symbol".to_string()),
                    })
                    .collect();
                let params = params?;

                let body = if items.len() == 3 {
                    items[2].clone()
                } else {
                    Value::List(
                        std::iter::once(Value::Symbol("do".to_string()))
                            .chain(items[2..].iter().cloned())
                            .collect(),
                    )
                };

                let func = Value::Fn {
                    params,
                    body: Box::new(body),
                    env: env.clone(),
                };
                env.define(name, func);
                Ok(Value::Nil)
            } else {
                Err("function name must be a symbol".to_string())
            }
        }
        _ => Err("define requires a symbol or function signature".to_string()),
    }
}

fn eval_set(items: &[Value], env: &Env) -> Result<Value, String> {
    if items.len() != 3 {
        return Err("set! requires exactly 2 arguments".to_string());
    }
    if let Value::Symbol(name) = &items[1] {
        let value = eval(&items[2], env)?;
        env.set(name, value)?;
        Ok(Value::Nil)
    } else {
        Err("set! requires a symbol".to_string())
    }
}

fn eval_let(items: &[Value], env: &Env) -> Result<Value, String> {
    if items.len() < 2 {
        return Err("let requires at least 1 argument".to_string());
    }

    let bindings = match &items[1] {
        Value::List(b) => b,
        _ => return Err("let bindings must be a list".to_string()),
    };

    let local_env = Env::with_parent(env);

    for binding in bindings {
        if let Value::List(pair) = binding {
            if pair.len() != 2 {
                return Err("let binding must be (name value)".to_string());
            }
            if let Value::Symbol(name) = &pair[0] {
                let value = eval(&pair[1], env)?;
                local_env.define(name, value);
            } else {
                return Err("let binding name must be a symbol".to_string());
            }
        } else {
            return Err("let binding must be a list".to_string());
        }
    }

    let mut result = Value::Nil;
    for expr in &items[2..] {
        result = eval(expr, &local_env)?;
    }
    Ok(result)
}

fn eval_fn(items: &[Value], env: &Env) -> Result<Value, String> {
    if items.len() < 3 {
        return Err("fn requires at least 2 arguments".to_string());
    }

    let params = match &items[1] {
        Value::List(p) => p
            .iter()
            .map(|x| match x {
                Value::Symbol(s) => Ok(s.clone()),
                _ => Err("parameter must be a symbol".to_string()),
            })
            .collect::<Result<Vec<String>, String>>()?,
        _ => return Err("fn parameters must be a list".to_string()),
    };

    let body = if items.len() == 3 {
        items[2].clone()
    } else {
        Value::List(
            std::iter::once(Value::Symbol("do".to_string()))
                .chain(items[2..].iter().cloned())
                .collect(),
        )
    };

    Ok(Value::Fn {
        params,
        body: Box::new(body),
        env: env.clone(),
    })
}

fn eval_do(items: &[Value], env: &Env) -> Result<Value, String> {
    let mut result = Value::Nil;
    for expr in &items[1..] {
        result = eval(expr, env)?;
    }
    Ok(result)
}

fn eval_and(items: &[Value], env: &Env) -> Result<Value, String> {
    let mut result = Value::Bool(true);
    for expr in &items[1..] {
        result = eval(expr, env)?;
        if !result.is_truthy() {
            return Ok(result);
        }
    }
    Ok(result)
}

fn eval_or(items: &[Value], env: &Env) -> Result<Value, String> {
    for expr in &items[1..] {
        let result = eval(expr, env)?;
        if result.is_truthy() {
            return Ok(result);
        }
    }
    Ok(Value::Bool(false))
}

fn eval_load(items: &[Value], env: &Env) -> Result<Value, String> {
    if items.len() != 2 {
        return Err("load requires exactly 1 argument".to_string());
    }

    let path = match eval(&items[1], env)? {
        Value::String(s) => s,
        other => return Err(format!("load: expected string path, got {}", other.type_name())),
    };

    let contents = fs::read_to_string(Path::new(&path))
        .map_err(|e| format!("load: cannot read '{}': {}", path, e))?;

    let exprs = parse(&contents).map_err(|e| format!("load: parse error in '{}': {}", path, e))?;

    let mut result = Value::Nil;
    for expr in &exprs {
        result = eval(expr, env)?;
    }
    Ok(result)
}

fn eval_trace_on() -> Result<Value, String> {
    TRACE_ENABLED.with(|t| *t.borrow_mut() = true);
    Ok(Value::Nil)
}

fn eval_trace_off() -> Result<Value, String> {
    TRACE_ENABLED.with(|t| *t.borrow_mut() = false);
    Ok(Value::Nil)
}

fn trace_enter(expr: &Value) {
    TRACE_ENABLED.with(|enabled| {
        if *enabled.borrow() {
            TRACE_DEPTH.with(|depth| {
                let d = *depth.borrow();
                let indent = "  ".repeat(d);
                eprintln!("{}> {}", indent, expr);
                *depth.borrow_mut() = d + 1;
            });
        }
    });
}

fn trace_exit(result: &Result<Value, String>) {
    TRACE_ENABLED.with(|enabled| {
        if *enabled.borrow() {
            TRACE_DEPTH.with(|depth| {
                let d = depth.borrow().saturating_sub(1);
                *depth.borrow_mut() = d;
                let indent = "  ".repeat(d);
                match result {
                    Ok(v) => eprintln!("{}< {}", indent, v),
                    Err(e) => eprintln!("{}! {}", indent, e),
                }
            });
        }
    });
}
