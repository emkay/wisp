use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::env::Env;
use crate::value::{native_fn, Value};

pub fn load_stdlib(env: &Env) {
    // Arithmetic
    env.define("+", native_fn(add));
    env.define("-", native_fn(sub));
    env.define("*", native_fn(mul));
    env.define("/", native_fn(div));
    env.define("mod", native_fn(modulo));

    // Comparison
    env.define("=", native_fn(eq));
    env.define("<", native_fn(lt));
    env.define(">", native_fn(gt));
    env.define("<=", native_fn(lte));
    env.define(">=", native_fn(gte));

    // Numeric conversions
    env.define("floor", native_fn(floor_fn));
    env.define("ceil", native_fn(ceil_fn));
    env.define("round", native_fn(round_fn));
    env.define("int", native_fn(to_int));

    // Logic
    env.define("not", native_fn(not));

    // List operations
    env.define("list", native_fn(list));
    env.define("car", native_fn(car));
    env.define("cdr", native_fn(cdr));
    env.define("cons", native_fn(cons));
    env.define("null?", native_fn(null_p));
    env.define("length", native_fn(length));
    env.define("list-ref", native_fn(list_ref));
    env.define("append", native_fn(append));

    // Type predicates
    env.define("nil?", native_fn(nil_p));
    env.define("bool?", native_fn(bool_p));
    env.define("int?", native_fn(int_p));
    env.define("float?", native_fn(float_p));
    env.define("string?", native_fn(string_p));
    env.define("symbol?", native_fn(symbol_p));
    env.define("list?", native_fn(list_p));
    env.define("fn?", native_fn(fn_p));

    // I/O
    env.define("println", native_fn(println_fn));
    env.define("print", native_fn(print_fn));

    // String operations
    env.define("string-append", native_fn(string_append));
    env.define("symbol->string", native_fn(symbol_to_string));
    env.define("string->symbol", native_fn(string_to_symbol));

    // Hash map operations
    env.define("hash", native_fn(hash_new));
    env.define("hash-get", native_fn(hash_get));
    env.define("hash-set!", native_fn(hash_set));
    env.define("hash-keys", native_fn(hash_keys));
    env.define("hash?", native_fn(hash_p));
}

// Helpers for numeric operations
fn to_number(v: &Value) -> Result<(f64, bool), String> {
    match v {
        Value::Int(n) => Ok((*n as f64, true)),
        Value::Float(n) => {
            // Reject NaN and Infinity as inputs
            if !n.is_finite() {
                return Err(format!("invalid number: {}", n));
            }
            Ok((*n, false))
        }
        _ => Err(format!("expected number, got {}", v.type_name())),
    }
}

/// Check that a computed result is finite (not NaN or Infinity)
fn check_finite(n: f64, op: &str) -> Result<f64, String> {
    if n.is_nan() {
        Err(format!("{}: result is not a number (NaN)", op))
    } else if n.is_infinite() {
        Err(format!("{}: result is infinite (overflow)", op))
    } else {
        Ok(n)
    }
}

/// Maximum f64 value that can be safely converted to i64.
/// i64::MAX (9223372036854775807) cannot be exactly represented as f64,
/// so we use the largest f64 that is <= i64::MAX.
const MAX_SAFE_INT: f64 = 9223372036854774784.0; // 2^63 - 1024

/// Minimum f64 value that can be safely converted to i64.
/// i64::MIN (-9223372036854775808) CAN be exactly represented as f64.
const MIN_SAFE_INT: f64 = -9223372036854775808.0; // -2^63

/// Safely convert f64 to i64, returning Float if out of range.
/// Properly handles NaN, Infinity, and boundary cases.
fn f64_to_int_value(n: f64) -> Value {
    // NaN and Infinity stay as Float (will be caught by validation elsewhere)
    if !n.is_finite() {
        return Value::Float(n);
    }

    // Check if within safe conversion range
    if (MIN_SAFE_INT..=MAX_SAFE_INT).contains(&n) {
        Value::Int(n as i64)
    } else {
        Value::Float(n)
    }
}

/// Helper for add/mul - fold over args with given identity and operation
fn fold_numeric(
    args: &[Value],
    identity: f64,
    op: fn(f64, f64) -> f64,
    op_name: &str,
) -> Result<Value, String> {
    let mut acc = identity;
    let mut all_int = true;

    for arg in args {
        let (n, is_int) = to_number(arg)?;
        acc = op(acc, n);
        all_int = all_int && is_int;
    }

    // Check for overflow (Infinity) or invalid operations (NaN)
    check_finite(acc, op_name)?;

    if all_int {
        Ok(f64_to_int_value(acc))
    } else {
        Ok(Value::Float(acc))
    }
}

fn add(args: Vec<Value>) -> Result<Value, String> {
    fold_numeric(&args, 0.0, |a, b| a + b, "+")
}

fn mul(args: Vec<Value>) -> Result<Value, String> {
    fold_numeric(&args, 1.0, |a, b| a * b, "*")
}

fn sub(args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("- requires at least 1 argument".to_string());
    }

    let (first, first_is_int) = to_number(&args[0])?;

    // Unary minus
    if args.len() == 1 {
        return if first_is_int {
            Ok(f64_to_int_value(-first))
        } else {
            Ok(Value::Float(-first))
        };
    }

    // Binary minus: first - rest
    let mut result = first;
    let mut all_int = first_is_int;

    for arg in &args[1..] {
        let (n, is_int) = to_number(arg)?;
        result -= n;
        all_int = all_int && is_int;
    }

    // Check for overflow
    check_finite(result, "-")?;

    if all_int {
        Ok(f64_to_int_value(result))
    } else {
        Ok(Value::Float(result))
    }
}

fn div(args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("/ requires at least 1 argument".to_string());
    }

    let (first, _) = to_number(&args[0])?;

    if args.len() == 1 {
        if first == 0.0 {
            return Err("/: division by zero".to_string());
        }
        let result = 1.0 / first;
        check_finite(result, "/")?;
        return Ok(Value::Float(result));
    }

    let mut result = first;

    for arg in &args[1..] {
        let (n, _) = to_number(arg)?;
        if n == 0.0 {
            return Err("/: division by zero".to_string());
        }
        result /= n;
    }

    check_finite(result, "/")?;
    Ok(Value::Float(result))
}

fn modulo(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("mod requires exactly 2 arguments".to_string());
    }
    match (&args[0], &args[1]) {
        (Value::Int(a), Value::Int(b)) => {
            if *b == 0 {
                Err("mod: division by zero".to_string())
            } else {
                Ok(Value::Int(a % b))
            }
        }
        _ => {
            let (a, _) = to_number(&args[0])?;
            let (b, _) = to_number(&args[1])?;
            if b == 0.0 {
                Err("mod: division by zero".to_string())
            } else {
                let result = a % b;
                check_finite(result, "mod")?;
                Ok(Value::Float(result))
            }
        }
    }
}

fn floor_fn(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("floor requires 1 argument".to_string());
    }
    match &args[0] {
        Value::Int(n) => Ok(Value::Int(*n)),
        Value::Float(n) => Ok(f64_to_int_value(n.floor())),
        _ => Err(format!("floor: expected number, got {}", args[0].type_name())),
    }
}

fn ceil_fn(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("ceil requires 1 argument".to_string());
    }
    match &args[0] {
        Value::Int(n) => Ok(Value::Int(*n)),
        Value::Float(n) => Ok(f64_to_int_value(n.ceil())),
        _ => Err(format!("ceil: expected number, got {}", args[0].type_name())),
    }
}

fn round_fn(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("round requires 1 argument".to_string());
    }
    match &args[0] {
        Value::Int(n) => Ok(Value::Int(*n)),
        Value::Float(n) => Ok(f64_to_int_value(n.round())),
        _ => Err(format!("round: expected number, got {}", args[0].type_name())),
    }
}

fn to_int(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("int requires 1 argument".to_string());
    }
    match &args[0] {
        Value::Int(n) => Ok(Value::Int(*n)),
        Value::Float(n) => Ok(f64_to_int_value(n.trunc())),
        _ => Err(format!("int: expected number, got {}", args[0].type_name())),
    }
}

fn eq(args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("= requires at least 2 arguments".to_string());
    }
    for i in 1..args.len() {
        if args[i - 1] != args[i] {
            return Ok(Value::Bool(false));
        }
    }
    Ok(Value::Bool(true))
}

fn lt(args: Vec<Value>) -> Result<Value, String> {
    compare_chain(&args, |a, b| a < b)
}

fn gt(args: Vec<Value>) -> Result<Value, String> {
    compare_chain(&args, |a, b| a > b)
}

fn lte(args: Vec<Value>) -> Result<Value, String> {
    compare_chain(&args, |a, b| a <= b)
}

fn gte(args: Vec<Value>) -> Result<Value, String> {
    compare_chain(&args, |a, b| a >= b)
}

fn compare_chain<F: Fn(f64, f64) -> bool>(args: &[Value], cmp: F) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("comparison requires at least 2 arguments".to_string());
    }
    let mut prev = to_number(&args[0])?.0;
    for arg in &args[1..] {
        let curr = to_number(arg)?.0;
        if !cmp(prev, curr) {
            return Ok(Value::Bool(false));
        }
        prev = curr;
    }
    Ok(Value::Bool(true))
}

fn not(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("not requires exactly 1 argument".to_string());
    }
    Ok(Value::Bool(!args[0].is_truthy()))
}

fn list(args: Vec<Value>) -> Result<Value, String> {
    Ok(Value::List(args))
}

fn car(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("car requires exactly 1 argument".to_string());
    }
    match &args[0] {
        Value::List(items) if !items.is_empty() => Ok(items[0].clone()),
        Value::List(_) => Err("car: empty list".to_string()),
        _ => Err(format!("car: expected list, got {}", args[0].type_name())),
    }
}

fn cdr(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("cdr requires exactly 1 argument".to_string());
    }
    match &args[0] {
        Value::List(items) if !items.is_empty() => Ok(Value::List(items[1..].to_vec())),
        Value::List(_) => Err("cdr: empty list".to_string()),
        _ => Err(format!("cdr: expected list, got {}", args[0].type_name())),
    }
}

fn cons(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("cons requires exactly 2 arguments".to_string());
    }
    match &args[1] {
        Value::List(items) => {
            let mut new_list = vec![args[0].clone()];
            new_list.extend(items.iter().cloned());
            Ok(Value::List(new_list))
        }
        _ => Err(format!(
            "cons: expected list as second argument, got {}",
            args[1].type_name()
        )),
    }
}

fn null_p(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("null? requires exactly 1 argument".to_string());
    }
    Ok(Value::Bool(matches!(&args[0], Value::List(items) if items.is_empty())))
}

fn length(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("length requires exactly 1 argument".to_string());
    }
    match &args[0] {
        Value::List(items) => Ok(Value::Int(items.len() as i64)),
        Value::String(s) => Ok(Value::Int(s.len() as i64)),
        _ => Err(format!(
            "length: expected list or string, got {}",
            args[0].type_name()
        )),
    }
}

fn list_ref(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("list-ref requires exactly 2 arguments".to_string());
    }
    match (&args[0], &args[1]) {
        (Value::List(items), Value::Int(i)) => {
            if *i < 0 {
                return Err(format!("list-ref: negative index {}", i));
            }
            let idx = *i as usize;
            if idx < items.len() {
                Ok(items[idx].clone())
            } else {
                Err(format!("list-ref: index {} out of bounds (length {})", i, items.len()))
            }
        }
        _ => Err("list-ref: expected (list int)".to_string()),
    }
}

fn append(args: Vec<Value>) -> Result<Value, String> {
    let mut result = Vec::new();
    for arg in args {
        match arg {
            Value::List(items) => result.extend(items),
            _ => return Err(format!("append: expected list, got {}", arg.type_name())),
        }
    }
    Ok(Value::List(result))
}

// Type predicates
fn nil_p(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("nil? requires exactly 1 argument".to_string());
    }
    Ok(Value::Bool(matches!(args[0], Value::Nil)))
}

fn bool_p(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("bool? requires exactly 1 argument".to_string());
    }
    Ok(Value::Bool(matches!(args[0], Value::Bool(_))))
}

fn int_p(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("int? requires exactly 1 argument".to_string());
    }
    Ok(Value::Bool(matches!(args[0], Value::Int(_))))
}

fn float_p(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("float? requires exactly 1 argument".to_string());
    }
    Ok(Value::Bool(matches!(args[0], Value::Float(_))))
}

fn string_p(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("string? requires exactly 1 argument".to_string());
    }
    Ok(Value::Bool(matches!(args[0], Value::String(_))))
}

fn symbol_p(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("symbol? requires exactly 1 argument".to_string());
    }
    Ok(Value::Bool(matches!(args[0], Value::Symbol(_))))
}

fn list_p(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("list? requires exactly 1 argument".to_string());
    }
    Ok(Value::Bool(matches!(args[0], Value::List(_))))
}

fn fn_p(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("fn? requires exactly 1 argument".to_string());
    }
    Ok(Value::Bool(matches!(
        args[0],
        Value::Fn { .. } | Value::NativeFn(_)
    )))
}

// I/O
fn print_impl(args: &[Value]) {
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            print!(" ");
        }
        match arg {
            Value::String(s) => print!("{}", s),
            other => print!("{}", other),
        }
    }
}

fn println_fn(args: Vec<Value>) -> Result<Value, String> {
    print_impl(&args);
    println!();
    Ok(Value::Nil)
}

fn print_fn(args: Vec<Value>) -> Result<Value, String> {
    print_impl(&args);
    Ok(Value::Nil)
}

// String operations
fn string_append(args: Vec<Value>) -> Result<Value, String> {
    let mut result = String::new();
    for arg in args {
        match arg {
            Value::String(s) => result.push_str(&s),
            _ => return Err(format!("string-append: expected string, got {}", arg.type_name())),
        }
    }
    Ok(Value::String(result))
}

fn symbol_to_string(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("symbol->string requires exactly 1 argument".to_string());
    }
    match &args[0] {
        Value::Symbol(s) => Ok(Value::String(s.clone())),
        _ => Err(format!(
            "symbol->string: expected symbol, got {}",
            args[0].type_name()
        )),
    }
}

fn string_to_symbol(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("string->symbol requires exactly 1 argument".to_string());
    }
    match &args[0] {
        Value::String(s) => Ok(Value::Symbol(s.clone())),
        _ => Err(format!(
            "string->symbol: expected string, got {}",
            args[0].type_name()
        )),
    }
}

// Hash map operations

// (hash) -> empty hash map
// (hash "key1" val1 "key2" val2 ...) -> hash map with entries
fn hash_new(args: Vec<Value>) -> Result<Value, String> {
    if !args.len().is_multiple_of(2) {
        return Err("hash requires an even number of arguments (key-value pairs)".to_string());
    }

    let mut map = HashMap::new();
    for chunk in args.chunks(2) {
        let key = match &chunk[0] {
            Value::String(s) => s.clone(),
            _ => return Err(format!("hash: keys must be strings, got {}", chunk[0].type_name())),
        };
        map.insert(key, chunk[1].clone());
    }

    Ok(Value::HashMap(Rc::new(RefCell::new(map))))
}

// (hash-get hash key) -> value or nil
// (hash-get hash key default) -> value or default
fn hash_get(args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 2 || args.len() > 3 {
        return Err("hash-get requires 2-3 arguments".to_string());
    }

    let map = match &args[0] {
        Value::HashMap(m) => m.borrow(),
        _ => return Err(format!("hash-get: expected hash, got {}", args[0].type_name())),
    };

    let key = match &args[1] {
        Value::String(s) => s,
        _ => return Err(format!("hash-get: key must be string, got {}", args[1].type_name())),
    };

    match map.get(key) {
        Some(v) => Ok(v.clone()),
        None => {
            if args.len() == 3 {
                Ok(args[2].clone())
            } else {
                Ok(Value::Nil)
            }
        }
    }
}

// (hash-set! hash key value) -> nil (mutates hash)
fn hash_set(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 3 {
        return Err("hash-set! requires 3 arguments".to_string());
    }

    let map = match &args[0] {
        Value::HashMap(m) => m,
        _ => return Err(format!("hash-set!: expected hash, got {}", args[0].type_name())),
    };

    let key = match &args[1] {
        Value::String(s) => s.clone(),
        _ => return Err(format!("hash-set!: key must be string, got {}", args[1].type_name())),
    };

    map.borrow_mut().insert(key, args[2].clone());
    Ok(Value::Nil)
}

// (hash-keys hash) -> list of keys
fn hash_keys(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("hash-keys requires 1 argument".to_string());
    }

    let map = match &args[0] {
        Value::HashMap(m) => m.borrow(),
        _ => return Err(format!("hash-keys: expected hash, got {}", args[0].type_name())),
    };

    let keys: Vec<Value> = map.keys().map(|k| Value::String(k.clone())).collect();
    Ok(Value::List(keys))
}

// (hash? val) -> bool
fn hash_p(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("hash? requires 1 argument".to_string());
    }
    Ok(Value::Bool(matches!(args[0], Value::HashMap(_))))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create a Float value
    fn float(n: f64) -> Value {
        Value::Float(n)
    }

    // Helper to create an Int value
    fn int(n: i64) -> Value {
        Value::Int(n)
    }

    // ===== f64_to_int_value tests =====

    #[test]
    fn test_f64_to_int_value_normal() {
        assert_eq!(f64_to_int_value(42.0), int(42));
        assert_eq!(f64_to_int_value(-100.0), int(-100));
        assert_eq!(f64_to_int_value(0.0), int(0));
    }

    #[test]
    fn test_f64_to_int_value_nan_returns_float() {
        let result = f64_to_int_value(f64::NAN);
        match result {
            Value::Float(n) => assert!(n.is_nan()),
            _ => panic!("expected Float for NaN"),
        }
    }

    #[test]
    fn test_f64_to_int_value_infinity_returns_float() {
        assert!(matches!(f64_to_int_value(f64::INFINITY), Value::Float(n) if n.is_infinite()));
        assert!(matches!(f64_to_int_value(f64::NEG_INFINITY), Value::Float(n) if n.is_infinite()));
    }

    #[test]
    fn test_f64_to_int_value_large_values_return_float() {
        // Values beyond safe i64 range should stay as Float
        let huge = 1e19;
        assert!(matches!(f64_to_int_value(huge), Value::Float(_)));
        assert!(matches!(f64_to_int_value(-huge), Value::Float(_)));
    }

    #[test]
    fn test_f64_to_int_value_boundary() {
        // i64::MIN can be exactly represented as f64
        assert_eq!(f64_to_int_value(i64::MIN as f64), int(i64::MIN));

        // MAX_SAFE_INT should convert to int
        assert!(matches!(f64_to_int_value(MAX_SAFE_INT), Value::Int(_)));

        // Just above MAX_SAFE_INT should stay as float
        let above_max = MAX_SAFE_INT + 2048.0;
        assert!(matches!(f64_to_int_value(above_max), Value::Float(_)));
    }

    // ===== to_number validation tests =====

    #[test]
    fn test_to_number_rejects_nan() {
        let result = to_number(&float(f64::NAN));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid number"));
    }

    #[test]
    fn test_to_number_rejects_infinity() {
        let result = to_number(&float(f64::INFINITY));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid number"));

        let result = to_number(&float(f64::NEG_INFINITY));
        assert!(result.is_err());
    }

    #[test]
    fn test_to_number_accepts_normal_float() {
        let result = to_number(&float(3.14));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), (3.14, false));
    }

    #[test]
    fn test_to_number_accepts_int() {
        let result = to_number(&int(42));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), (42.0, true));
    }

    // ===== Overflow detection tests =====

    #[test]
    fn test_mul_overflow_detected() {
        // 1e308 * 10 overflows to Infinity
        let result = mul(vec![float(1e308), float(10.0)]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("infinite"));
    }

    #[test]
    fn test_add_overflow_detected() {
        // f64::MAX + f64::MAX overflows
        let result = add(vec![float(f64::MAX), float(f64::MAX)]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("infinite"));
    }

    #[test]
    fn test_sub_overflow_detected() {
        // -f64::MAX - f64::MAX overflows to -Infinity
        let result = sub(vec![float(-f64::MAX), float(f64::MAX)]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("infinite"));
    }

    // ===== Division tests =====

    #[test]
    fn test_div_by_zero_error() {
        let result = div(vec![int(1), int(0)]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("division by zero"));
    }

    #[test]
    fn test_div_by_zero_float_error() {
        let result = div(vec![float(1.0), float(0.0)]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("division by zero"));
    }

    #[test]
    fn test_mod_by_zero_error() {
        let result = modulo(vec![int(10), int(0)]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("division by zero"));
    }

    #[test]
    fn test_div_normal() {
        let result = div(vec![float(10.0), float(2.0)]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), float(5.0));
    }

    // ===== Normal arithmetic tests =====

    #[test]
    fn test_add_integers_stay_integer() {
        let result = add(vec![int(1), int(2), int(3)]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), int(6));
    }

    #[test]
    fn test_add_mixed_becomes_float() {
        let result = add(vec![int(1), float(2.5)]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), float(3.5));
    }

    #[test]
    fn test_mul_integers() {
        let result = mul(vec![int(2), int(3), int(4)]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), int(24));
    }

    #[test]
    fn test_sub_binary() {
        let result = sub(vec![int(10), int(3)]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), int(7));
    }

    #[test]
    fn test_sub_unary() {
        let result = sub(vec![int(5)]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), int(-5));
    }

    #[test]
    fn test_modulo_integers() {
        let result = modulo(vec![int(10), int(3)]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), int(1));
    }
}
