use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::value::Value;

/// [`Env`] is scope for Wisp variables. It maps variable names to values. The property `inner` holds
/// a `Rc<RefCell<T>>`. The [`Rc`] allows us to share the reference safely and the [`RefCell`] allows us to have mutable state for an `Env`. This means that
/// multiple owners can have the same `Env`. The way this works is the [`Rc`], or reference counter, keeps track of the number of times [`Env`] has been cloned and dropped. An important thing to note is that when [`Env`] is cloned new memory isn't allocated. The new cloned item will reference the same allocation as the original. When can ensure that [`Env`] is going to stick around for until everything goes out of scope because each time an item does go out of scope and is dropped the counter is decremented. When there are none left then it's safe to release because we know nothing else is sharing it. This is **not thread safe**, but that probably doesn't matter for now. The [`RefCell`] is needed because it allows for the mutation of that state.
/// This is so we can define new variables in Wisp and keep them in a [lexical scope](https://en.wikipedia.org/wiki/Scope_(computer_programming)).
#[derive(Clone)]
pub struct Env {
    inner: Rc<RefCell<EnvInner>>,
}

struct EnvInner {
    bindings: HashMap<String, Value>,
    parent: Option<Env>,
}

impl Env {
    /// Create a brand new [`Env`] that has no parent. This would be the start of the scope.
    pub fn new() -> Self {
        Env {
            inner: Rc::new(RefCell::new(EnvInner {
                bindings: HashMap::new(),
                parent: None,
            })),
        }
    }

    /// Create an [`Env`] that has a parent that is a reference to another [`Env`].
    pub fn with_parent(parent: &Env) -> Self {
        Env {
            inner: Rc::new(RefCell::new(EnvInner {
                bindings: HashMap::new(),
                parent: Some(parent.clone()),
            })),
        }
    }

    /// Recurse through the scope to see if there is a binding of `name` to a value in the scope.
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

    /// Create a new variable in the scope.
    pub fn define(&self, name: &str, value: Value) {
        self.inner.borrow_mut().bindings.insert(name.to_string(), value);
    }

    /// Set a new value to a variable.
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
