use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[cfg(not(target_arch = "wasm32"))]
use std::fs;

use crate::env::Env;
use crate::parse::{parse, Expr, Span};
use crate::value::Value;

thread_local! {
    static TRACE_ENABLED: RefCell<bool> = const { RefCell::new(false) };
    static TRACE_DEPTH: RefCell<usize> = const { RefCell::new(0) };
    static SCRIPT_DIR: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    /// Cache for preloaded script files (used in WASM)
    static SCRIPT_CACHE: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
}

/// Store a preloaded script in the cache (for WASM)
pub fn cache_script(path: &str, contents: String) {
    SCRIPT_CACHE.with(|c| c.borrow_mut().insert(path.to_string(), contents));
}

/// Get a cached script
fn get_cached_script(path: &str) -> Option<String> {
    SCRIPT_CACHE.with(|c| c.borrow().get(path).cloned())
}

/// Set the base directory for resolving relative paths in `load`
pub fn set_script_dir(path: &Path) {
    let dir = path.parent().map(|p| p.to_path_buf());
    SCRIPT_DIR.with(|d| *d.borrow_mut() = dir);
}

/// Resolve a path relative to the current script directory.
///
/// Security: This function prevents path traversal attacks by:
/// - Rejecting absolute paths
/// - Canonicalizing the resolved path
/// - Verifying the result stays within the script directory
pub fn resolve_path(path: &str) -> Result<PathBuf, String> {
    let p = Path::new(path);

    // Reject absolute paths
    if p.is_absolute() {
        return Err(format!(
            "absolute paths are not allowed: '{}'",
            path
        ));
    }

    SCRIPT_DIR.with(|d| {
        let base = match &*d.borrow() {
            Some(base) => base.clone(),
            None => {
                // No script directory set - allow relative paths from current dir
                // but still validate them
                std::env::current_dir().map_err(|e| format!("cannot get current directory: {}", e))?
            }
        };

        let joined = base.join(p);

        // Canonicalize both paths to resolve .., symlinks, etc.
        let canonical_base = base.canonicalize().map_err(|e| {
            format!("cannot canonicalize base path '{}': {}", base.display(), e)
        })?;

        let canonical_path = joined.canonicalize().map_err(|e| {
            format!("cannot resolve path '{}': {}", joined.display(), e)
        })?;

        // Verify the resolved path is within the base directory
        if !canonical_path.starts_with(&canonical_base) {
            return Err(format!(
                "path '{}' escapes the script directory",
                path
            ));
        }

        Ok(canonical_path)
    })
}

/// Evaluate a Wisp expression in the given environment.
///
/// # Evaluation Rules
/// - Self-evaluating values (nil, bool, int, float, string) return themselves
/// - Symbols are looked up in the environment
/// - Empty lists evaluate to nil
/// - Lists are either special forms or function calls
///
/// # Special Forms
/// Special forms are detected by checking if the first element is a symbol
/// matching: quote, if, cond, define, set!, let, fn, lambda, do, begin,
/// and, or, load, trace-on, trace-off
pub fn eval(expr: &Expr, env: &Env) -> Result<Value, String> {
    eval_impl(&expr.value, expr.span, env)
}

/// Evaluate a Value directly (used for sub-expressions within lists)
pub fn eval_value(value: &Value, env: &Env) -> Result<Value, String> {
    eval_impl(value, None, env)
}

fn eval_impl(value: &Value, span: Option<Span>, env: &Env) -> Result<Value, String> {
    match value {
        Value::Nil | Value::Bool(_) | Value::Int(_) | Value::Float(_) | Value::String(_) => {
            Ok(value.clone())
        }

        Value::Symbol(name) => env
            .get(name)
            .ok_or_else(|| with_span(format!("undefined variable: {}", name), span)),

        Value::List(items) if items.is_empty() => Ok(Value::Nil),

        Value::List(items) => {
            let first = &items[0];

            // Check for special forms
            if let Value::Symbol(name) = first {
                match name.as_str() {
                    "quote" => return eval_quote(items, span),
                    "if" => return eval_if(items, span, env),
                    "cond" => return eval_cond(items, span, env),
                    "define" => return eval_define(items, span, env),
                    "set!" => return eval_set(items, span, env),
                    "let" => return eval_let(items, span, env),
                    "fn" | "lambda" => return eval_fn(items, span, env),
                    "do" | "begin" => return eval_do(items, env),
                    "and" => return eval_and(items, env),
                    "or" => return eval_or(items, env),
                    "load" => return eval_load(items, span, env),
                    "trace-on" => return eval_trace_on(),
                    "trace-off" => return eval_trace_off(),
                    _ => {}
                }
            }

            // Function call - use parent span for better error messages
            let func = eval_impl(first, span, env)?;
            let args: Result<Vec<Value>, String> =
                items[1..].iter().map(|arg| eval_impl(arg, span, env)).collect();
            let args = args?;

            trace_enter(value);
            let result = apply(&func, args, span);
            trace_exit(&result);
            result
        }

        // Runtime values (functions, hashmaps) evaluate to themselves
        Value::Fn { .. } | Value::NativeFn(_) | Value::HashMap(_) => Ok(value.clone()),
    }
}

/// Format an error message with optional span
fn with_span(msg: String, span: Option<Span>) -> String {
    match span {
        Some(s) => format!("{} at {}", msg, s),
        None => msg,
    }
}

/// Apply a function to arguments.
///
/// For user-defined functions, creates a new environment with the closure's
/// environment as parent, binds parameters to arguments, then evaluates the body.
pub fn apply(func: &Value, args: Vec<Value>, span: Option<Span>) -> Result<Value, String> {
    match func {
        Value::NativeFn(f) => f(args).map_err(|e| with_span(e, span)),
        Value::Fn { params, body, env } => {
            if args.len() != params.len() {
                return Err(with_span(
                    format!("expected {} arguments, got {}", params.len(), args.len()),
                    span,
                ));
            }
            let local_env = Env::with_parent(env);
            for (param, arg) in params.iter().zip(args.into_iter()) {
                local_env.define(param, arg);
            }
            eval_value(body, &local_env)
        }
        _ => Err(with_span(format!("not a function: {}", func), span)),
    }
}

fn eval_quote(items: &[Value], span: Option<Span>) -> Result<Value, String> {
    if items.len() != 2 {
        return Err(with_span("quote requires exactly 1 argument".to_string(), span));
    }
    Ok(items[1].clone())
}

fn eval_if(items: &[Value], span: Option<Span>, env: &Env) -> Result<Value, String> {
    if items.len() < 3 || items.len() > 4 {
        return Err(with_span("if requires 2 or 3 arguments".to_string(), span));
    }
    let cond = eval_value(&items[1], env)?;
    if cond.is_truthy() {
        eval_value(&items[2], env)
    } else if items.len() == 4 {
        eval_value(&items[3], env)
    } else {
        Ok(Value::Nil)
    }
}

fn eval_cond(items: &[Value], span: Option<Span>, env: &Env) -> Result<Value, String> {
    for clause in &items[1..] {
        if let Value::List(parts) = clause {
            if parts.is_empty() {
                return Err(with_span("cond clause cannot be empty".to_string(), span));
            }

            let test = if let Value::Symbol(s) = &parts[0] {
                s == "else"
            } else {
                false
            };

            if test || eval_value(&parts[0], env)?.is_truthy() {
                let mut result = Value::Nil;
                for expr in &parts[1..] {
                    result = eval_value(expr, env)?;
                }
                return Ok(result);
            }
        } else {
            return Err(with_span("cond clause must be a list".to_string(), span));
        }
    }
    Ok(Value::Nil)
}

/// Evaluate a define expression.
///
/// Supports two forms:
/// - `(define name value)` - bind value to name
/// - `(define (name params...) body...)` - function definition shorthand
///
/// For functions with multiple body expressions, they are wrapped in a `do` block.
fn eval_define(items: &[Value], span: Option<Span>, env: &Env) -> Result<Value, String> {
    if items.len() < 2 {
        return Err(with_span("define requires at least 1 argument".to_string(), span));
    }

    match &items[1] {
        // (define x 10)
        Value::Symbol(name) => {
            if items.len() != 3 {
                return Err(with_span("define requires exactly 2 arguments".to_string(), span));
            }
            let value = eval_value(&items[2], env)?;
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
                        _ => Err(with_span("parameter must be a symbol".to_string(), span)),
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
                Err(with_span("function name must be a symbol".to_string(), span))
            }
        }
        _ => Err(with_span("define requires a symbol or function signature".to_string(), span)),
    }
}

fn eval_set(items: &[Value], span: Option<Span>, env: &Env) -> Result<Value, String> {
    if items.len() != 3 {
        return Err(with_span("set! requires exactly 2 arguments".to_string(), span));
    }
    if let Value::Symbol(name) = &items[1] {
        let value = eval_value(&items[2], env)?;
        env.set(name, value).map_err(|e| with_span(e, span))?;
        Ok(Value::Nil)
    } else {
        Err(with_span("set! requires a symbol".to_string(), span))
    }
}

fn eval_let(items: &[Value], span: Option<Span>, env: &Env) -> Result<Value, String> {
    if items.len() < 2 {
        return Err(with_span("let requires at least 1 argument".to_string(), span));
    }

    let bindings = match &items[1] {
        Value::List(b) => b,
        _ => return Err(with_span("let bindings must be a list".to_string(), span)),
    };

    let local_env = Env::with_parent(env);

    for binding in bindings {
        if let Value::List(pair) = binding {
            if pair.len() != 2 {
                return Err(with_span("let binding must be (name value)".to_string(), span));
            }
            if let Value::Symbol(name) = &pair[0] {
                let value = eval_value(&pair[1], env)?;
                local_env.define(name, value);
            } else {
                return Err(with_span("let binding name must be a symbol".to_string(), span));
            }
        } else {
            return Err(with_span("let binding must be a list".to_string(), span));
        }
    }

    let mut result = Value::Nil;
    for expr in &items[2..] {
        result = eval_value(expr, &local_env)?;
    }
    Ok(result)
}

fn eval_fn(items: &[Value], span: Option<Span>, env: &Env) -> Result<Value, String> {
    if items.len() < 3 {
        return Err(with_span("fn requires at least 2 arguments".to_string(), span));
    }

    let params = match &items[1] {
        Value::List(p) => p
            .iter()
            .map(|x| match x {
                Value::Symbol(s) => Ok(s.clone()),
                _ => Err(with_span("parameter must be a symbol".to_string(), span)),
            })
            .collect::<Result<Vec<String>, String>>()?,
        _ => return Err(with_span("fn parameters must be a list".to_string(), span)),
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
        result = eval_value(expr, env)?;
    }
    Ok(result)
}

fn eval_and(items: &[Value], env: &Env) -> Result<Value, String> {
    let mut result = Value::Bool(true);
    for expr in &items[1..] {
        result = eval_value(expr, env)?;
        if !result.is_truthy() {
            return Ok(result);
        }
    }
    Ok(result)
}

fn eval_or(items: &[Value], env: &Env) -> Result<Value, String> {
    for expr in &items[1..] {
        let result = eval_value(expr, env)?;
        if result.is_truthy() {
            return Ok(result);
        }
    }
    Ok(Value::Bool(false))
}

fn eval_load(items: &[Value], span: Option<Span>, env: &Env) -> Result<Value, String> {
    if items.len() != 2 {
        return Err(with_span("load requires exactly 1 argument".to_string(), span));
    }

    let path_arg = match eval_value(&items[1], env)? {
        Value::String(s) => s,
        other => return Err(with_span(format!("load: expected string path, got {}", other.type_name()), span)),
    };

    // Resolve path relative to current script directory (with security validation)
    let resolved = resolve_path(&path_arg)
        .map_err(|e| with_span(format!("load: {}", e), span))?;

    // Try to get from cache first (for WASM preloaded scripts)
    let contents = if let Some(cached) = get_cached_script(&path_arg) {
        cached
    } else {
        #[cfg(not(target_arch = "wasm32"))]
        {
            fs::read_to_string(&resolved)
                .map_err(|e| with_span(format!("load: cannot read '{}': {}", resolved.display(), e), span))?
        }
        #[cfg(target_arch = "wasm32")]
        {
            return Err(with_span(format!("load: '{}' was not preloaded", path_arg), span));
        }
    };

    let exprs = parse(&contents).map_err(|e| format!("load: parse error in '{}': {}", resolved.display(), e))?;

    // Save current script dir, set new one for nested loads
    let old_dir = SCRIPT_DIR.with(|d| d.borrow().clone());
    if let Some(parent) = resolved.parent() {
        SCRIPT_DIR.with(|d| *d.borrow_mut() = Some(parent.to_path_buf()));
    }

    let mut result = Value::Nil;
    for expr in &exprs {
        result = eval(expr, env)?;
    }

    // Restore previous script dir
    SCRIPT_DIR.with(|d| *d.borrow_mut() = old_dir);

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::TempDir;

    fn with_temp_script_dir<F, R>(f: F) -> R
    where
        F: FnOnce(&Path) -> R,
    {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let base = temp_dir.path();

        // Set up the script directory
        SCRIPT_DIR.with(|d| *d.borrow_mut() = Some(base.to_path_buf()));

        let result = f(base);

        // Clean up
        SCRIPT_DIR.with(|d| *d.borrow_mut() = None);

        result
    }

    #[test]
    fn test_resolve_path_rejects_absolute_paths() {
        with_temp_script_dir(|_| {
            let result = resolve_path("/etc/passwd");
            assert!(result.is_err());
            assert!(result.unwrap_err().contains("absolute paths are not allowed"));
        });
    }

    #[test]
    fn test_resolve_path_rejects_path_traversal() {
        with_temp_script_dir(|base| {
            // Create a file inside the temp dir so canonicalize works
            let inner_dir = base.join("subdir");
            fs::create_dir(&inner_dir).unwrap();

            // Create a file outside that we'll try to escape to
            let outside_file = base.parent().unwrap().join("outside.txt");
            File::create(&outside_file).unwrap();

            // Try to escape using ..
            let result = resolve_path("../outside.txt");
            assert!(result.is_err());
            assert!(result.unwrap_err().contains("escapes the script directory"));

            // Clean up the outside file
            let _ = fs::remove_file(outside_file);
        });
    }

    #[test]
    fn test_resolve_path_rejects_sneaky_traversal() {
        with_temp_script_dir(|base| {
            // Create structure: base/subdir/
            let inner_dir = base.join("subdir");
            fs::create_dir(&inner_dir).unwrap();

            // Create a file outside
            let outside_file = base.parent().unwrap().join("secret.txt");
            File::create(&outside_file).unwrap();

            // Try sneaky traversal: subdir/../../secret.txt
            let result = resolve_path("subdir/../../secret.txt");
            assert!(result.is_err());
            assert!(result.unwrap_err().contains("escapes the script directory"));

            let _ = fs::remove_file(outside_file);
        });
    }

    #[test]
    fn test_resolve_path_allows_valid_relative_paths() {
        with_temp_script_dir(|base| {
            // Create a valid file
            let valid_file = base.join("script.wisp");
            File::create(&valid_file)
                .unwrap()
                .write_all(b"(define x 1)")
                .unwrap();

            let result = resolve_path("script.wisp");
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), valid_file.canonicalize().unwrap());
        });
    }

    #[test]
    fn test_resolve_path_allows_subdirectory_paths() {
        with_temp_script_dir(|base| {
            // Create a subdirectory with a file
            let subdir = base.join("lib");
            fs::create_dir(&subdir).unwrap();
            let lib_file = subdir.join("utils.wisp");
            File::create(&lib_file)
                .unwrap()
                .write_all(b"(define y 2)")
                .unwrap();

            let result = resolve_path("lib/utils.wisp");
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), lib_file.canonicalize().unwrap());
        });
    }

    #[test]
    fn test_resolve_path_allows_dotdot_within_sandbox() {
        with_temp_script_dir(|base| {
            // Create: base/a/b/ and base/a/file.wisp
            let dir_a = base.join("a");
            let dir_b = dir_a.join("b");
            fs::create_dir_all(&dir_b).unwrap();

            let file_in_a = dir_a.join("file.wisp");
            File::create(&file_in_a)
                .unwrap()
                .write_all(b"(define z 3)")
                .unwrap();

            // From base, accessing a/b/../file.wisp should work (stays in sandbox)
            let result = resolve_path("a/b/../file.wisp");
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), file_in_a.canonicalize().unwrap());
        });
    }

    #[test]
    fn test_resolve_path_rejects_nonexistent_files() {
        with_temp_script_dir(|_| {
            // Canonicalize fails for nonexistent files
            let result = resolve_path("nonexistent.wisp");
            assert!(result.is_err());
            assert!(result.unwrap_err().contains("cannot resolve path"));
        });
    }
}
