use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::value::Value;

#[derive(Clone)]
pub struct Env {
    inner: Rc<RefCell<EnvInner>>,
}

struct EnvInner {
    bindings: HashMap<String, Value>,
    parent: Option<Env>,
}

impl Env {
    pub fn new() -> Self {
        Env {
            inner: Rc::new(RefCell::new(EnvInner {
                bindings: HashMap::new(),
                parent: None,
            })),
        }
    }

    pub fn with_parent(parent: &Env) -> Self {
        Env {
            inner: Rc::new(RefCell::new(EnvInner {
                bindings: HashMap::new(),
                parent: Some(parent.clone()),
            })),
        }
    }

    pub fn get(&self, name: &str) -> Option<Value> {
        let inner = self.inner.borrow();
        if let Some(value) = inner.bindings.get(name) {
            Some(value.clone())
        } else if let Some(ref parent) = inner.parent {
            parent.get(name)
        } else {
            None
        }
    }

    pub fn define(&self, name: &str, value: Value) {
        self.inner.borrow_mut().bindings.insert(name.to_string(), value);
    }

    pub fn set(&self, name: &str, value: Value) -> Result<(), String> {
        {
            let mut inner = self.inner.borrow_mut();
            if inner.bindings.contains_key(name) {
                inner.bindings.insert(name.to_string(), value);
                return Ok(());
            }
        }
        let inner = self.inner.borrow();
        if let Some(ref parent) = inner.parent {
            let parent = parent.clone();
            drop(inner);
            parent.set(name, value)
        } else {
            Err(format!("undefined variable: {}", name))
        }
    }
}

impl Default for Env {
    fn default() -> Self {
        Self::new()
    }
}
