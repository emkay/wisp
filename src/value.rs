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
            Value::Int(n) => Ok(*n as u32),
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
