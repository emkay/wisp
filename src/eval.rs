use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[cfg(not(target_arch = "wasm32"))]
use std::fs;

use crate::env::Env;
use crate::parse::{parse, Expr, Span};
use crate::value::Value;

/// Result of evaluation - either a final value or a tail call to continue
enum EvalResult {
    /// Evaluation is complete with this value
    Done(Value),
    /// Tail call: continue evaluating with this function application
    TailCall {
        func: Value,
        args: Vec<Value>,
        span: Option<Span>,
    },
}

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
    trampoline(&expr.value, expr.span, env)
}

/// Evaluate a Value directly (used for sub-expressions within lists)
pub fn eval_value(value: &Value, env: &Env) -> Result<Value, String> {
    trampoline(value, None, env)
}

/// Trampoline loop for tail call optimization.
/// Continues evaluating until we get a final value (not a tail call).
fn trampoline(value: &Value, span: Option<Span>, env: &Env) -> Result<Value, String> {
    let mut current_value = value.clone();
    let mut current_span = span;
    let mut current_env = env.clone();

    loop {
        match eval_impl(&current_value, current_span, &current_env, true)? {
            EvalResult::Done(v) => return Ok(v),
            EvalResult::TailCall { func, args, span: call_span } => {
                // Handle the tail call by setting up for the next iteration
                match func {
                    Value::Fn { params, body, env: fn_env } => {
                        if args.len() != params.len() {
                            return Err(with_span(
                                format!("expected {} arguments, got {}", params.len(), args.len()),
                                call_span,
                            ));
                        }
                        // Create new environment and bind parameters
                        let local_env = Env::with_parent(&fn_env);
                        for (param, arg) in params.iter().zip(args.into_iter()) {
                            local_env.define(param, arg);
                        }
                        // Continue with the function body
                        current_value = (*body).clone();
                        current_span = call_span;
                        current_env = local_env;
                    }
                    Value::NativeFn(f) => {
                        // Native functions can't be tail-called in the trampoline sense,
                        // just call them directly and return
                        return f(args).map_err(|e| with_span(e, call_span));
                    }
                    _ => {
                        return Err(with_span(format!("not a function: {}", func), call_span));
                    }
                }
            }
        }
    }
}

/// Internal evaluation that returns EvalResult for TCO support.
/// The `tail` parameter indicates whether this expression is in tail position.
fn eval_impl(value: &Value, span: Option<Span>, env: &Env, tail: bool) -> Result<EvalResult, String> {
    match value {
        Value::Nil | Value::Bool(_) | Value::Int(_) | Value::Float(_) | Value::String(_) => {
            Ok(EvalResult::Done(value.clone()))
        }

        Value::Symbol(name) => env
            .get(name)
            .map(EvalResult::Done)
            .ok_or_else(|| with_span(format!("undefined variable: {}", name), span)),

        Value::List(items) if items.is_empty() => Ok(EvalResult::Done(Value::Nil)),

        Value::List(items) => {
            let first = &items[0];

            // Check for special forms
            if let Value::Symbol(name) = first {
                match name.as_str() {
                    "quote" => return eval_quote(items, span),
                    "if" => return eval_if(items, span, env, tail),
                    "cond" => return eval_cond(items, span, env, tail),
                    "define" => return eval_define(items, span, env),
                    "set!" => return eval_set(items, span, env),
                    "let" => return eval_let(items, span, env, tail),
                    "fn" | "lambda" => return eval_fn(items, span, env),
                    "do" | "begin" => return eval_do(items, span, env, tail),
                    "and" => return eval_and(items, span, env, tail),
                    "or" => return eval_or(items, span, env, tail),
                    "load" => return eval_load(items, span, env),
                    "trace-on" => return eval_trace_on(),
                    "trace-off" => return eval_trace_off(),
                    _ => {}
                }
            }

            // Function call - evaluate function and arguments (not in tail position)
            let func = eval_impl(first, span, env, false)?.into_value();
            let args: Result<Vec<Value>, String> = items[1..]
                .iter()
                .map(|arg| eval_impl(arg, span, env, false).map(|r| r.into_value()))
                .collect();
            let args = args?;

            // If we're in tail position, return a TailCall for the trampoline
            if tail {
                let was_tracing = trace_enter(value);
                if was_tracing {
                    // For tail calls with tracing, we need to handle exit specially
                    // The trace will be completed when the tail call returns
                    trace_exit(&Ok(Value::Symbol("<tail-call>".to_string())), was_tracing);
                }
                Ok(EvalResult::TailCall { func, args, span })
            } else {
                // Not in tail position - apply immediately
                let was_tracing = trace_enter(value);
                let result = apply(&func, args, span);
                trace_exit(&result, was_tracing);
                result.map(EvalResult::Done)
            }
        }

        // Runtime values (functions, hashmaps) evaluate to themselves
        Value::Fn { .. } | Value::NativeFn(_) | Value::HashMap(_) => {
            Ok(EvalResult::Done(value.clone()))
        }
    }
}

impl EvalResult {
    /// Extract the value, panicking if this is a TailCall.
    /// Used only in contexts where we know the result must be Done.
    fn into_value(self) -> Value {
        match self {
            EvalResult::Done(v) => v,
            EvalResult::TailCall { .. } => {
                panic!("unexpected tail call in non-tail position")
            }
        }
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

fn eval_quote(items: &[Value], span: Option<Span>) -> Result<EvalResult, String> {
    if items.len() != 2 {
        return Err(with_span("quote: requires exactly 1 argument".to_string(), span));
    }
    Ok(EvalResult::Done(items[1].clone()))
}

fn eval_if(items: &[Value], span: Option<Span>, env: &Env, tail: bool) -> Result<EvalResult, String> {
    if items.len() < 3 || items.len() > 4 {
        return Err(with_span("if: requires 2 or 3 arguments".to_string(), span));
    }
    // Condition is not in tail position
    let cond = eval_impl(&items[1], span, env, false)?.into_value();
    if cond.is_truthy() {
        // Then branch is in tail position
        eval_impl(&items[2], span, env, tail)
    } else if items.len() == 4 {
        // Else branch is in tail position
        eval_impl(&items[3], span, env, tail)
    } else {
        Ok(EvalResult::Done(Value::Nil))
    }
}

fn eval_cond(items: &[Value], span: Option<Span>, env: &Env, tail: bool) -> Result<EvalResult, String> {
    for clause in &items[1..] {
        if let Value::List(parts) = clause {
            if parts.is_empty() {
                return Err(with_span("cond: clause cannot be empty".to_string(), span));
            }

            let test = if let Value::Symbol(s) = &parts[0] {
                s == "else"
            } else {
                false
            };

            // Condition is not in tail position
            if test || eval_impl(&parts[0], span, env, false)?.into_value().is_truthy() {
                // Evaluate all but the last expression (not in tail position)
                let body_exprs = &parts[1..];
                if body_exprs.is_empty() {
                    return Ok(EvalResult::Done(Value::Nil));
                }
                for expr in &body_exprs[..body_exprs.len() - 1] {
                    eval_impl(expr, span, env, false)?;
                }
                // Last expression is in tail position
                return eval_impl(&body_exprs[body_exprs.len() - 1], span, env, tail);
            }
        } else {
            return Err(with_span("cond: clause must be a list".to_string(), span));
        }
    }
    Ok(EvalResult::Done(Value::Nil))
}

/// Evaluate a define expression.
///
/// Supports two forms:
/// - `(define name value)` - bind value to name
/// - `(define (name params...) body...)` - function definition shorthand
///
/// For functions with multiple body expressions, they are wrapped in a `do` block.
fn eval_define(items: &[Value], span: Option<Span>, env: &Env) -> Result<EvalResult, String> {
    if items.len() < 2 {
        return Err(with_span("define: requires at least 1 argument".to_string(), span));
    }

    match &items[1] {
        // (define x 10)
        Value::Symbol(name) => {
            if items.len() != 3 {
                return Err(with_span("define: requires exactly 2 arguments".to_string(), span));
            }
            // Value expression is not in tail position (define returns nil)
            let value = eval_impl(&items[2], span, env, false)?.into_value();
            env.define(name, value);
            Ok(EvalResult::Done(Value::Nil))
        }
        // (define (f x y) body)
        Value::List(sig) if !sig.is_empty() => {
            if let Value::Symbol(name) = &sig[0] {
                let params: Result<Vec<String>, String> = sig[1..]
                    .iter()
                    .map(|p| match p {
                        Value::Symbol(s) => Ok(s.clone()),
                        _ => Err(with_span("define: parameter must be a symbol".to_string(), span)),
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
                Ok(EvalResult::Done(Value::Nil))
            } else {
                Err(with_span("define: function name must be a symbol".to_string(), span))
            }
        }
        _ => Err(with_span("define: requires a symbol or function signature".to_string(), span)),
    }
}

fn eval_set(items: &[Value], span: Option<Span>, env: &Env) -> Result<EvalResult, String> {
    if items.len() != 3 {
        return Err(with_span("set!: requires exactly 2 arguments".to_string(), span));
    }
    if let Value::Symbol(name) = &items[1] {
        // Value expression is not in tail position (set! returns nil)
        let value = eval_impl(&items[2], span, env, false)?.into_value();
        env.set(name, value).map_err(|e| with_span(e, span))?;
        Ok(EvalResult::Done(Value::Nil))
    } else {
        Err(with_span("set!: requires a symbol".to_string(), span))
    }
}

fn eval_let(items: &[Value], span: Option<Span>, env: &Env, tail: bool) -> Result<EvalResult, String> {
    if items.len() < 2 {
        return Err(with_span("let: requires at least 1 argument".to_string(), span));
    }

    let bindings = match &items[1] {
        Value::List(b) => b,
        _ => return Err(with_span("let: bindings must be a list".to_string(), span)),
    };

    let local_env = Env::with_parent(env);

    for binding in bindings {
        if let Value::List(pair) = binding {
            if pair.len() != 2 {
                return Err(with_span("let: binding must be (name value)".to_string(), span));
            }
            if let Value::Symbol(name) = &pair[0] {
                // Binding values are not in tail position
                let value = eval_impl(&pair[1], span, env, false)?.into_value();
                local_env.define(name, value);
            } else {
                return Err(with_span("let: binding name must be a symbol".to_string(), span));
            }
        } else {
            return Err(with_span("let: binding must be a list".to_string(), span));
        }
    }

    // Evaluate body expressions
    let body_exprs = &items[2..];
    if body_exprs.is_empty() {
        return Ok(EvalResult::Done(Value::Nil));
    }
    // All but the last are not in tail position
    for expr in &body_exprs[..body_exprs.len() - 1] {
        eval_impl(expr, span, &local_env, false)?;
    }
    // Last expression is in tail position
    eval_impl(&body_exprs[body_exprs.len() - 1], span, &local_env, tail)
}

fn eval_fn(items: &[Value], span: Option<Span>, env: &Env) -> Result<EvalResult, String> {
    if items.len() < 3 {
        return Err(with_span("fn: requires at least 2 arguments".to_string(), span));
    }

    let params = match &items[1] {
        Value::List(p) => p
            .iter()
            .map(|x| match x {
                Value::Symbol(s) => Ok(s.clone()),
                _ => Err(with_span("fn: parameter must be a symbol".to_string(), span)),
            })
            .collect::<Result<Vec<String>, String>>()?,
        _ => return Err(with_span("fn: parameters must be a list".to_string(), span)),
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

    Ok(EvalResult::Done(Value::Fn {
        params,
        body: Box::new(body),
        env: env.clone(),
    }))
}

fn eval_do(items: &[Value], span: Option<Span>, env: &Env, tail: bool) -> Result<EvalResult, String> {
    let body_exprs = &items[1..];
    if body_exprs.is_empty() {
        return Ok(EvalResult::Done(Value::Nil));
    }
    // All but the last are not in tail position
    for expr in &body_exprs[..body_exprs.len() - 1] {
        eval_impl(expr, span, env, false)?;
    }
    // Last expression is in tail position
    eval_impl(&body_exprs[body_exprs.len() - 1], span, env, tail)
}

fn eval_and(items: &[Value], span: Option<Span>, env: &Env, tail: bool) -> Result<EvalResult, String> {
    let exprs = &items[1..];
    if exprs.is_empty() {
        return Ok(EvalResult::Done(Value::Bool(true)));
    }
    // All but the last are not in tail position (short-circuit evaluation)
    for expr in &exprs[..exprs.len() - 1] {
        let result = eval_impl(expr, span, env, false)?.into_value();
        if !result.is_truthy() {
            return Ok(EvalResult::Done(result));
        }
    }
    // Last expression is in tail position
    eval_impl(&exprs[exprs.len() - 1], span, env, tail)
}

fn eval_or(items: &[Value], span: Option<Span>, env: &Env, tail: bool) -> Result<EvalResult, String> {
    let exprs = &items[1..];
    if exprs.is_empty() {
        return Ok(EvalResult::Done(Value::Bool(false)));
    }
    // All but the last are not in tail position (short-circuit evaluation)
    for expr in &exprs[..exprs.len() - 1] {
        let result = eval_impl(expr, span, env, false)?.into_value();
        if result.is_truthy() {
            return Ok(EvalResult::Done(result));
        }
    }
    // Last expression is in tail position
    eval_impl(&exprs[exprs.len() - 1], span, env, tail)
}

fn eval_load(items: &[Value], span: Option<Span>, env: &Env) -> Result<EvalResult, String> {
    if items.len() != 2 {
        return Err(with_span("load: requires exactly 1 argument".to_string(), span));
    }

    let path_arg = match eval_impl(&items[1], span, env, false)?.into_value() {
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

    Ok(EvalResult::Done(result))
}

fn eval_trace_on() -> Result<EvalResult, String> {
    TRACE_ENABLED.with(|t| *t.borrow_mut() = true);
    Ok(EvalResult::Done(Value::Nil))
}

fn eval_trace_off() -> Result<EvalResult, String> {
    TRACE_ENABLED.with(|t| *t.borrow_mut() = false);
    Ok(EvalResult::Done(Value::Nil))
}

/// Enter a traced function call. Returns true if tracing was active (and depth was incremented).
fn trace_enter(expr: &Value) -> bool {
    TRACE_ENABLED.with(|enabled| {
        if *enabled.borrow() {
            TRACE_DEPTH.with(|depth| {
                let d = *depth.borrow();
                let indent = "  ".repeat(d);
                eprintln!("{}> {}", indent, expr);
                *depth.borrow_mut() = d + 1;
            });
            true
        } else {
            false
        }
    })
}

/// Exit a traced function call. Only decrements depth if `was_tracing` is true
/// (i.e., if trace_enter actually incremented the depth).
fn trace_exit(result: &Result<Value, String>, was_tracing: bool) {
    if !was_tracing {
        return;
    }

    TRACE_DEPTH.with(|depth| {
        let d = depth.borrow().saturating_sub(1);
        *depth.borrow_mut() = d;

        // Only print if tracing is still enabled
        TRACE_ENABLED.with(|enabled| {
            if *enabled.borrow() {
                let indent = "  ".repeat(d);
                match result {
                    Ok(v) => eprintln!("{}< {}", indent, v),
                    Err(e) => eprintln!("{}! {}", indent, e),
                }
            }
        });
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

    // ===== Trace depth tests =====

    fn get_trace_depth() -> usize {
        TRACE_DEPTH.with(|d| *d.borrow())
    }

    fn set_trace_depth(depth: usize) {
        TRACE_DEPTH.with(|d| *d.borrow_mut() = depth);
    }

    fn set_trace_enabled(enabled: bool) {
        TRACE_ENABLED.with(|t| *t.borrow_mut() = enabled);
    }

    #[test]
    fn test_trace_depth_increments_when_enabled() {
        set_trace_depth(0);
        set_trace_enabled(true);

        let was_tracing = trace_enter(&Value::Int(42));

        assert!(was_tracing);
        assert_eq!(get_trace_depth(), 1);

        // Clean up
        set_trace_enabled(false);
        set_trace_depth(0);
    }

    #[test]
    fn test_trace_depth_not_incremented_when_disabled() {
        set_trace_depth(0);
        set_trace_enabled(false);

        let was_tracing = trace_enter(&Value::Int(42));

        assert!(!was_tracing);
        assert_eq!(get_trace_depth(), 0);
    }

    #[test]
    fn test_trace_exit_decrements_when_was_tracing() {
        set_trace_depth(1);
        set_trace_enabled(true);

        trace_exit(&Ok(Value::Int(42)), true);

        assert_eq!(get_trace_depth(), 0);

        // Clean up
        set_trace_enabled(false);
    }

    #[test]
    fn test_trace_exit_decrements_even_when_tracing_disabled_midway() {
        // Simulate: tracing was on when we entered, but turned off before exit
        set_trace_depth(1);
        set_trace_enabled(false); // Tracing disabled now

        // was_tracing=true means we DID increment on enter
        trace_exit(&Ok(Value::Int(42)), true);

        // Depth should still be decremented
        assert_eq!(get_trace_depth(), 0);
    }

    #[test]
    fn test_trace_exit_does_not_decrement_when_was_not_tracing() {
        set_trace_depth(0);
        set_trace_enabled(false);

        // was_tracing=false means we did NOT increment on enter
        trace_exit(&Ok(Value::Int(42)), false);

        // Depth should stay at 0, not underflow
        assert_eq!(get_trace_depth(), 0);
    }

    #[test]
    fn test_trace_depth_balanced_after_error() {
        set_trace_depth(0);
        set_trace_enabled(true);

        let was_tracing = trace_enter(&Value::Int(42));
        assert_eq!(get_trace_depth(), 1);

        // Simulate an error result
        trace_exit(&Err("some error".to_string()), was_tracing);

        // Depth should be back to 0
        assert_eq!(get_trace_depth(), 0);

        // Clean up
        set_trace_enabled(false);
    }

    // ===== Tail Call Optimization tests =====

    use crate::stdlib::load_stdlib;

    fn eval_string(code: &str) -> Result<Value, String> {
        let env = Env::new();
        load_stdlib(&env);
        let exprs = parse(code).map_err(|e| format!("parse error: {}", e))?;
        let mut result = Value::Nil;
        for expr in &exprs {
            result = eval(expr, &env)?;
        }
        Ok(result)
    }

    #[test]
    fn test_tco_deep_tail_recursion() {
        // This would overflow the stack without TCO
        // count-down is a simple tail-recursive function
        let code = r#"
            (define (count-down n)
              (if (<= n 0)
                  0
                  (count-down (- n 1))))
            (count-down 10000)
        "#;
        let result = eval_string(code);
        assert!(result.is_ok(), "TCO failed: {:?}", result);
        assert_eq!(result.unwrap(), Value::Int(0));
    }

    #[test]
    fn test_tco_tail_recursive_sum() {
        // Tail-recursive sum with accumulator
        let code = r#"
            (define (sum-acc n acc)
              (if (<= n 0)
                  acc
                  (sum-acc (- n 1) (+ acc n))))
            (sum-acc 1000 0)
        "#;
        let result = eval_string(code);
        assert!(result.is_ok(), "TCO failed: {:?}", result);
        // Sum of 1..1000 = 1000 * 1001 / 2 = 500500
        assert_eq!(result.unwrap(), Value::Int(500500));
    }

    #[test]
    fn test_tco_mutual_recursion() {
        // Mutual tail recursion: is-even? and is-odd?
        let code = r#"
            (define (is-even? n)
              (if (= n 0)
                  true
                  (is-odd? (- n 1))))
            (define (is-odd? n)
              (if (= n 0)
                  false
                  (is-even? (- n 1))))
            (is-even? 10000)
        "#;
        let result = eval_string(code);
        assert!(result.is_ok(), "TCO mutual recursion failed: {:?}", result);
        assert_eq!(result.unwrap(), Value::Bool(true));
    }

    #[test]
    fn test_tco_in_cond() {
        // Tail call in cond clause
        let code = r#"
            (define (classify n)
              (cond
                ((< n 0) (classify (- 0 n)))
                ((= n 0) 'zero)
                ((< n 10) 'small)
                (else (classify (- n 10)))))
            (classify 10005)
        "#;
        let result = eval_string(code);
        assert!(result.is_ok(), "TCO in cond failed: {:?}", result);
        assert_eq!(result.unwrap(), Value::Symbol("small".to_string()));
    }

    #[test]
    fn test_tco_in_let() {
        // Tail call in let body
        let code = r#"
            (define (factorial-helper n acc)
              (if (<= n 1)
                  acc
                  (let ((next (- n 1))
                        (new-acc (* acc n)))
                    (factorial-helper next new-acc))))
            (factorial-helper 20 1)
        "#;
        let result = eval_string(code);
        assert!(result.is_ok(), "TCO in let failed: {:?}", result);
        // 20! = 2432902008176640000
        assert_eq!(result.unwrap(), Value::Int(2432902008176640000));
    }

    #[test]
    fn test_tco_in_do() {
        // Tail call in do block
        let code = r#"
            (define (loop n)
              (if (<= n 0)
                  'done
                  (do
                    (+ 1 1)  ; side effect (non-tail)
                    (loop (- n 1)))))  ; tail call
            (loop 5000)
        "#;
        let result = eval_string(code);
        assert!(result.is_ok(), "TCO in do failed: {:?}", result);
        assert_eq!(result.unwrap(), Value::Symbol("done".to_string()));
    }

    #[test]
    fn test_tco_in_and() {
        // Tail call as last expression in and
        let code = r#"
            (define (check-and-recurse n)
              (if (<= n 0)
                  true
                  (and true (check-and-recurse (- n 1)))))
            (check-and-recurse 5000)
        "#;
        let result = eval_string(code);
        assert!(result.is_ok(), "TCO in and failed: {:?}", result);
        assert_eq!(result.unwrap(), Value::Bool(true));
    }

    #[test]
    fn test_tco_in_or() {
        // Tail call as last expression in or
        let code = r#"
            (define (check-or-recurse n)
              (if (<= n 0)
                  'found
                  (or false (check-or-recurse (- n 1)))))
            (check-or-recurse 5000)
        "#;
        let result = eval_string(code);
        assert!(result.is_ok(), "TCO in or failed: {:?}", result);
        assert_eq!(result.unwrap(), Value::Symbol("found".to_string()));
    }

    #[test]
    fn test_non_tail_call_still_works() {
        // Non-tail recursive call (can't go as deep, but should work for small n)
        let code = r#"
            (define (factorial n)
              (if (<= n 1)
                  1
                  (* n (factorial (- n 1)))))
            (factorial 10)
        "#;
        let result = eval_string(code);
        assert!(result.is_ok(), "Non-tail recursion failed: {:?}", result);
        assert_eq!(result.unwrap(), Value::Int(3628800));
    }
}
