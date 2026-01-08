use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

use crate::env::Env;

pub type NativeFn = Rc<dyn Fn(Vec<Value>) -> Result<Value, String>>;

#[derive(Clone)]
pub enum Value {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Symbol(String),
    List(Vec<Value>),
    Fn {
        params: Vec<String>,
        body: Box<Value>,
        env: Env,
    },
    NativeFn(NativeFn),
    HashMap(Rc<RefCell<HashMap<String, Value>>>),
}

/// Helper to wrap a native function for use in Wisp
pub fn native_fn(f: fn(Vec<Value>) -> Result<Value, String>) -> Value {
    Value::NativeFn(Rc::new(f))
}

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Nil => "nil",
            Value::Bool(_) => "bool",
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::String(_) => "string",
            Value::Symbol(_) => "symbol",
            Value::List(_) => "list",
            Value::Fn { .. } => "fn",
            Value::NativeFn(_) => "native-fn",
            Value::HashMap(_) => "hash-map",
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Nil => false,
            Value::Bool(b) => *b,
            _ => true,
        }
    }

    /// Extract a String, with context for error messages
    pub fn as_string(&self, ctx: &str) -> Result<String, String> {
        match self {
            Value::String(s) => Ok(s.clone()),
            _ => Err(format!("{}: expected string, got {}", ctx, self.type_name())),
        }
    }

    /// Extract an i64 integer
    pub fn as_int(&self, ctx: &str) -> Result<i64, String> {
        match self {
            Value::Int(n) => Ok(*n),
            _ => Err(format!("{}: expected integer, got {}", ctx, self.type_name())),
        }
    }

    /// Extract a u32 (for tile IDs, coordinates, etc.)
    pub fn as_u32(&self, ctx: &str) -> Result<u32, String> {
        match self {
            Value::Int(n) if *n >= 0 && *n <= u32::MAX as i64 => Ok(*n as u32),
            Value::Int(n) => Err(format!("{}: integer {} out of range for u32", ctx, n)),
            _ => Err(format!("{}: expected integer, got {}", ctx, self.type_name())),
        }
    }

    /// Extract a number as f32 (accepts both int and float)
    pub fn as_f32(&self, ctx: &str) -> Result<f32, String> {
        match self {
            Value::Int(n) => Ok(*n as f32),
            Value::Float(n) => Ok(*n as f32),
            _ => Err(format!("{}: expected number, got {}", ctx, self.type_name())),
        }
    }

    /// Extract a number as f64 (accepts both int and float)
    pub fn as_f64(&self, ctx: &str) -> Result<f64, String> {
        match self {
            Value::Int(n) => Ok(*n as f64),
            Value::Float(n) => Ok(*n),
            _ => Err(format!("{}: expected number, got {}", ctx, self.type_name())),
        }
    }

    /// Extract a list
    pub fn as_list(&self, ctx: &str) -> Result<&Vec<Value>, String> {
        match self {
            Value::List(items) => Ok(items),
            _ => Err(format!("{}: expected list, got {}", ctx, self.type_name())),
        }
    }

    /// Extract a symbol as &str
    pub fn as_symbol(&self, ctx: &str) -> Result<&str, String> {
        match self {
            Value::Symbol(s) => Ok(s),
            _ => Err(format!("{}: expected symbol, got {}", ctx, self.type_name())),
        }
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Nil => write!(f, "nil"),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Int(n) => write!(f, "{}", n),
            Value::Float(n) => write!(f, "{}", n),
            Value::String(s) => write!(f, "\"{}\"", s),
            Value::Symbol(s) => write!(f, "{}", s),
            Value::List(items) => {
                write!(f, "(")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, ")")
            }
            Value::Fn { params, .. } => {
                write!(f, "<fn ({})>", params.join(" "))
            }
            Value::NativeFn(_) => write!(f, "<native-fn>"),
            Value::HashMap(map) => {
                let map = map.borrow();
                write!(f, "{{")?;
                for (i, (k, v)) in map.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }
                    write!(f, "{}: {}", k, v)?;
                }
                write!(f, "}}")
            }
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Nil, Value::Nil) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Int(a), Value::Float(b)) => (*a as f64) == *b,
            (Value::Float(a), Value::Int(b)) => *a == (*b as f64),
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Symbol(a), Value::Symbol(b)) => a == b,
            (Value::List(a), Value::List(b)) => a == b,
            (Value::HashMap(a), Value::HashMap(b)) => *a.borrow() == *b.borrow(),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== type_name tests =====

    #[test]
    fn test_type_name_nil() {
        assert_eq!(Value::Nil.type_name(), "nil");
    }

    #[test]
    fn test_type_name_bool() {
        assert_eq!(Value::Bool(true).type_name(), "bool");
        assert_eq!(Value::Bool(false).type_name(), "bool");
    }

    #[test]
    fn test_type_name_int() {
        assert_eq!(Value::Int(42).type_name(), "int");
    }

    #[test]
    fn test_type_name_float() {
        assert_eq!(Value::Float(3.14).type_name(), "float");
    }

    #[test]
    fn test_type_name_string() {
        assert_eq!(Value::String("hello".to_string()).type_name(), "string");
    }

    #[test]
    fn test_type_name_symbol() {
        assert_eq!(Value::Symbol("x".to_string()).type_name(), "symbol");
    }

    #[test]
    fn test_type_name_list() {
        assert_eq!(Value::List(vec![]).type_name(), "list");
    }

    #[test]
    fn test_type_name_fn() {
        let f = Value::Fn {
            params: vec![],
            body: Box::new(Value::Nil),
            env: Env::new(),
        };
        assert_eq!(f.type_name(), "fn");
    }

    #[test]
    fn test_type_name_native_fn() {
        let f = native_fn(|_| Ok(Value::Nil));
        assert_eq!(f.type_name(), "native-fn");
    }

    #[test]
    fn test_type_name_hashmap() {
        let h = Value::HashMap(Rc::new(RefCell::new(HashMap::new())));
        assert_eq!(h.type_name(), "hash-map");
    }

    // ===== is_truthy tests =====

    #[test]
    fn test_is_truthy_nil() {
        assert!(!Value::Nil.is_truthy());
    }

    #[test]
    fn test_is_truthy_bool() {
        assert!(Value::Bool(true).is_truthy());
        assert!(!Value::Bool(false).is_truthy());
    }

    #[test]
    fn test_is_truthy_int() {
        assert!(Value::Int(0).is_truthy());  // 0 is truthy!
        assert!(Value::Int(1).is_truthy());
        assert!(Value::Int(-1).is_truthy());
    }

    #[test]
    fn test_is_truthy_float() {
        assert!(Value::Float(0.0).is_truthy());  // 0.0 is truthy!
        assert!(Value::Float(1.5).is_truthy());
    }

    #[test]
    fn test_is_truthy_string() {
        assert!(Value::String("".to_string()).is_truthy());  // Empty string is truthy!
        assert!(Value::String("hello".to_string()).is_truthy());
    }

    #[test]
    fn test_is_truthy_symbol() {
        assert!(Value::Symbol("x".to_string()).is_truthy());
    }

    #[test]
    fn test_is_truthy_list() {
        assert!(Value::List(vec![]).is_truthy());  // Empty list is truthy!
        assert!(Value::List(vec![Value::Int(1)]).is_truthy());
    }

    // ===== as_string tests =====

    #[test]
    fn test_as_string_success() {
        let v = Value::String("hello".to_string());
        assert_eq!(v.as_string("test"), Ok("hello".to_string()));
    }

    #[test]
    fn test_as_string_error() {
        let v = Value::Int(42);
        let result = v.as_string("my-fn");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("my-fn"));
        assert!(err.contains("expected string"));
        assert!(err.contains("got int"));
    }

    // ===== as_int tests =====

    #[test]
    fn test_as_int_success() {
        let v = Value::Int(42);
        assert_eq!(v.as_int("test"), Ok(42));
    }

    #[test]
    fn test_as_int_negative() {
        let v = Value::Int(-100);
        assert_eq!(v.as_int("test"), Ok(-100));
    }

    #[test]
    fn test_as_int_error_float() {
        let v = Value::Float(3.14);
        let result = v.as_int("test");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expected integer"));
    }

    // ===== as_u32 tests =====

    #[test]
    fn test_as_u32_success() {
        let v = Value::Int(100);
        assert_eq!(v.as_u32("test"), Ok(100));
    }

    #[test]
    fn test_as_u32_zero() {
        let v = Value::Int(0);
        assert_eq!(v.as_u32("test"), Ok(0));
    }

    #[test]
    fn test_as_u32_max() {
        let v = Value::Int(u32::MAX as i64);
        assert_eq!(v.as_u32("test"), Ok(u32::MAX));
    }

    #[test]
    fn test_as_u32_negative_error() {
        let v = Value::Int(-1);
        let result = v.as_u32("test");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("out of range"));
    }

    #[test]
    fn test_as_u32_too_large_error() {
        let v = Value::Int(u32::MAX as i64 + 1);
        let result = v.as_u32("test");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("out of range"));
    }

    #[test]
    fn test_as_u32_wrong_type_error() {
        let v = Value::Float(1.5);
        let result = v.as_u32("test");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expected integer"));
    }

    // ===== as_f32 tests =====

    #[test]
    fn test_as_f32_from_float() {
        let v = Value::Float(3.14);
        assert!((v.as_f32("test").unwrap() - 3.14).abs() < 0.001);
    }

    #[test]
    fn test_as_f32_from_int() {
        let v = Value::Int(42);
        assert_eq!(v.as_f32("test"), Ok(42.0));
    }

    #[test]
    fn test_as_f32_error() {
        let v = Value::String("nope".to_string());
        let result = v.as_f32("test");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expected number"));
    }

    // ===== as_f64 tests =====

    #[test]
    fn test_as_f64_from_float() {
        let v = Value::Float(3.14159);
        assert_eq!(v.as_f64("test"), Ok(3.14159));
    }

    #[test]
    fn test_as_f64_from_int() {
        let v = Value::Int(42);
        assert_eq!(v.as_f64("test"), Ok(42.0));
    }

    #[test]
    fn test_as_f64_error() {
        let v = Value::Bool(true);
        let result = v.as_f64("test");
        assert!(result.is_err());
    }

    // ===== as_list tests =====

    #[test]
    fn test_as_list_success() {
        let v = Value::List(vec![Value::Int(1), Value::Int(2)]);
        let list = v.as_list("test").unwrap();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_as_list_empty() {
        let v = Value::List(vec![]);
        let list = v.as_list("test").unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn test_as_list_error() {
        let v = Value::Int(42);
        let result = v.as_list("test");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expected list"));
    }

    // ===== as_symbol tests =====

    #[test]
    fn test_as_symbol_success() {
        let v = Value::Symbol("foo".to_string());
        assert_eq!(v.as_symbol("test"), Ok("foo"));
    }

    #[test]
    fn test_as_symbol_error() {
        let v = Value::String("not a symbol".to_string());
        let result = v.as_symbol("test");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expected symbol"));
    }

    // ===== Display tests =====

    #[test]
    fn test_display_nil() {
        assert_eq!(format!("{}", Value::Nil), "nil");
    }

    #[test]
    fn test_display_bool() {
        assert_eq!(format!("{}", Value::Bool(true)), "true");
        assert_eq!(format!("{}", Value::Bool(false)), "false");
    }

    #[test]
    fn test_display_int() {
        assert_eq!(format!("{}", Value::Int(42)), "42");
        assert_eq!(format!("{}", Value::Int(-17)), "-17");
    }

    #[test]
    fn test_display_float() {
        assert_eq!(format!("{}", Value::Float(3.5)), "3.5");
    }

    #[test]
    fn test_display_string() {
        assert_eq!(format!("{}", Value::String("hello".to_string())), "\"hello\"");
    }

    #[test]
    fn test_display_symbol() {
        assert_eq!(format!("{}", Value::Symbol("foo".to_string())), "foo");
    }

    #[test]
    fn test_display_empty_list() {
        assert_eq!(format!("{}", Value::List(vec![])), "()");
    }

    #[test]
    fn test_display_list() {
        let list = Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        assert_eq!(format!("{}", list), "(1 2 3)");
    }

    #[test]
    fn test_display_nested_list() {
        let inner = Value::List(vec![Value::Int(1), Value::Int(2)]);
        let outer = Value::List(vec![Value::Symbol("+".to_string()), inner]);
        assert_eq!(format!("{}", outer), "(+ (1 2))");
    }

    #[test]
    fn test_display_fn() {
        let f = Value::Fn {
            params: vec!["x".to_string(), "y".to_string()],
            body: Box::new(Value::Nil),
            env: Env::new(),
        };
        assert_eq!(format!("{}", f), "<fn (x y)>");
    }

    #[test]
    fn test_display_native_fn() {
        let f = native_fn(|_| Ok(Value::Nil));
        assert_eq!(format!("{}", f), "<native-fn>");
    }

    #[test]
    fn test_display_empty_hashmap() {
        let h = Value::HashMap(Rc::new(RefCell::new(HashMap::new())));
        assert_eq!(format!("{}", h), "{}");
    }

    // ===== PartialEq tests =====

    #[test]
    fn test_eq_nil() {
        assert_eq!(Value::Nil, Value::Nil);
    }

    #[test]
    fn test_eq_bool() {
        assert_eq!(Value::Bool(true), Value::Bool(true));
        assert_eq!(Value::Bool(false), Value::Bool(false));
        assert_ne!(Value::Bool(true), Value::Bool(false));
    }

    #[test]
    fn test_eq_int() {
        assert_eq!(Value::Int(42), Value::Int(42));
        assert_ne!(Value::Int(42), Value::Int(43));
    }

    #[test]
    fn test_eq_float() {
        assert_eq!(Value::Float(3.14), Value::Float(3.14));
        assert_ne!(Value::Float(3.14), Value::Float(3.15));
    }

    #[test]
    fn test_eq_int_float() {
        // Int and Float can be equal if values match
        assert_eq!(Value::Int(42), Value::Float(42.0));
        assert_eq!(Value::Float(100.0), Value::Int(100));
        assert_ne!(Value::Int(42), Value::Float(42.1));
    }

    #[test]
    fn test_eq_string() {
        assert_eq!(
            Value::String("hello".to_string()),
            Value::String("hello".to_string())
        );
        assert_ne!(
            Value::String("hello".to_string()),
            Value::String("world".to_string())
        );
    }

    #[test]
    fn test_eq_symbol() {
        assert_eq!(
            Value::Symbol("foo".to_string()),
            Value::Symbol("foo".to_string())
        );
        assert_ne!(
            Value::Symbol("foo".to_string()),
            Value::Symbol("bar".to_string())
        );
    }

    #[test]
    fn test_eq_list() {
        assert_eq!(
            Value::List(vec![Value::Int(1), Value::Int(2)]),
            Value::List(vec![Value::Int(1), Value::Int(2)])
        );
        assert_ne!(
            Value::List(vec![Value::Int(1)]),
            Value::List(vec![Value::Int(2)])
        );
        assert_ne!(
            Value::List(vec![Value::Int(1)]),
            Value::List(vec![Value::Int(1), Value::Int(2)])
        );
    }

    #[test]
    fn test_eq_empty_list() {
        assert_eq!(Value::List(vec![]), Value::List(vec![]));
    }

    #[test]
    fn test_ne_different_types() {
        assert_ne!(Value::Int(1), Value::Bool(true));
        assert_ne!(Value::String("1".to_string()), Value::Int(1));
        assert_ne!(Value::Symbol("nil".to_string()), Value::Nil);
        assert_ne!(Value::List(vec![]), Value::Nil);
    }

    #[test]
    fn test_eq_hashmap() {
        let mut m1 = HashMap::new();
        m1.insert("a".to_string(), Value::Int(1));

        let mut m2 = HashMap::new();
        m2.insert("a".to_string(), Value::Int(1));

        let h1 = Value::HashMap(Rc::new(RefCell::new(m1)));
        let h2 = Value::HashMap(Rc::new(RefCell::new(m2)));

        assert_eq!(h1, h2);
    }

    // ===== native_fn helper tests =====

    #[test]
    fn test_native_fn_helper() {
        fn my_fn(_args: Vec<Value>) -> Result<Value, String> {
            Ok(Value::Int(42))
        }

        let f = native_fn(my_fn);
        match f {
            Value::NativeFn(func) => {
                let result = func(vec![]);
                assert_eq!(result, Ok(Value::Int(42)));
            }
            _ => panic!("expected NativeFn"),
        }
    }

    #[test]
    fn test_native_fn_with_args() {
        fn add_ints(args: Vec<Value>) -> Result<Value, String> {
            let a = args[0].as_int("test")?;
            let b = args[1].as_int("test")?;
            Ok(Value::Int(a + b))
        }

        let f = native_fn(add_ints);
        match f {
            Value::NativeFn(func) => {
                let result = func(vec![Value::Int(10), Value::Int(20)]);
                assert_eq!(result, Ok(Value::Int(30)));
            }
            _ => panic!("expected NativeFn"),
        }
    }

    #[test]
    fn test_native_fn_error() {
        fn fail(_args: Vec<Value>) -> Result<Value, String> {
            Err("intentional error".to_string())
        }

        let f = native_fn(fail);
        match f {
            Value::NativeFn(func) => {
                let result = func(vec![]);
                assert!(result.is_err());
                assert!(result.unwrap_err().contains("intentional error"));
            }
            _ => panic!("expected NativeFn"),
        }
    }
}
