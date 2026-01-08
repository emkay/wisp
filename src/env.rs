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

    /// Set a new value to a variable. Iterative to avoid stack overflow with deep scopes.
    pub fn set(&self, name: &str, value: Value) -> Result<(), String> {
        let mut current = self.clone();
        loop {
            // Check if binding exists in current scope
            let found = current.inner.borrow().bindings.contains_key(name);
            if found {
                current
                    .inner
                    .borrow_mut()
                    .bindings
                    .insert(name.to_string(), value);
                return Ok(());
            }

            // Move to parent
            let parent = current.inner.borrow().parent.clone();
            match parent {
                Some(p) => current = p,
                None => return Err(format!("undefined variable: {}", name)),
            }
        }
    }
}

impl Default for Env {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_new_is_empty() {
        let env = Env::new();
        assert!(env.get("x").is_none());
    }

    #[test]
    fn test_env_define_and_get() {
        let env = Env::new();
        env.define("x", Value::Int(42));
        assert_eq!(env.get("x"), Some(Value::Int(42)));
    }

    #[test]
    fn test_env_define_multiple() {
        let env = Env::new();
        env.define("a", Value::Int(1));
        env.define("b", Value::Int(2));
        env.define("c", Value::Int(3));
        assert_eq!(env.get("a"), Some(Value::Int(1)));
        assert_eq!(env.get("b"), Some(Value::Int(2)));
        assert_eq!(env.get("c"), Some(Value::Int(3)));
    }

    #[test]
    fn test_env_redefine_in_same_scope() {
        let env = Env::new();
        env.define("x", Value::Int(1));
        env.define("x", Value::Int(2));
        assert_eq!(env.get("x"), Some(Value::Int(2)));
    }

    #[test]
    fn test_env_with_parent() {
        let parent = Env::new();
        parent.define("x", Value::Int(42));

        let child = Env::with_parent(&parent);
        // Child can see parent's bindings
        assert_eq!(child.get("x"), Some(Value::Int(42)));
    }

    #[test]
    fn test_env_child_shadows_parent() {
        let parent = Env::new();
        parent.define("x", Value::Int(1));

        let child = Env::with_parent(&parent);
        child.define("x", Value::Int(2));

        // Child sees its own binding
        assert_eq!(child.get("x"), Some(Value::Int(2)));
        // Parent still has original
        assert_eq!(parent.get("x"), Some(Value::Int(1)));
    }

    #[test]
    fn test_env_three_levels() {
        let root = Env::new();
        root.define("a", Value::Int(1));

        let middle = Env::with_parent(&root);
        middle.define("b", Value::Int(2));

        let leaf = Env::with_parent(&middle);
        leaf.define("c", Value::Int(3));

        // Leaf can see all
        assert_eq!(leaf.get("a"), Some(Value::Int(1)));
        assert_eq!(leaf.get("b"), Some(Value::Int(2)));
        assert_eq!(leaf.get("c"), Some(Value::Int(3)));

        // Middle can see root and itself
        assert_eq!(middle.get("a"), Some(Value::Int(1)));
        assert_eq!(middle.get("b"), Some(Value::Int(2)));
        assert!(middle.get("c").is_none());

        // Root only sees itself
        assert_eq!(root.get("a"), Some(Value::Int(1)));
        assert!(root.get("b").is_none());
        assert!(root.get("c").is_none());
    }

    #[test]
    fn test_env_set_in_current_scope() {
        let env = Env::new();
        env.define("x", Value::Int(1));
        assert!(env.set("x", Value::Int(2)).is_ok());
        assert_eq!(env.get("x"), Some(Value::Int(2)));
    }

    #[test]
    fn test_env_set_in_parent_scope() {
        let parent = Env::new();
        parent.define("x", Value::Int(1));

        let child = Env::with_parent(&parent);
        // set! should update parent's binding
        assert!(child.set("x", Value::Int(2)).is_ok());

        // Both see the new value
        assert_eq!(child.get("x"), Some(Value::Int(2)));
        assert_eq!(parent.get("x"), Some(Value::Int(2)));
    }

    #[test]
    fn test_env_set_prefers_closer_scope() {
        let parent = Env::new();
        parent.define("x", Value::Int(1));

        let child = Env::with_parent(&parent);
        child.define("x", Value::Int(10)); // Shadow

        // set! should update child's binding, not parent's
        assert!(child.set("x", Value::Int(20)).is_ok());

        assert_eq!(child.get("x"), Some(Value::Int(20)));
        assert_eq!(parent.get("x"), Some(Value::Int(1))); // Parent unchanged
    }

    #[test]
    fn test_env_set_undefined_error() {
        let env = Env::new();
        let result = env.set("undefined", Value::Int(1));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("undefined variable"));
    }

    #[test]
    fn test_env_set_undefined_with_parent() {
        let parent = Env::new();
        let child = Env::with_parent(&parent);
        let result = child.set("undefined", Value::Int(1));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("undefined variable"));
    }

    #[test]
    fn test_env_clone_shares_bindings() {
        let env1 = Env::new();
        env1.define("x", Value::Int(1));

        let env2 = env1.clone();
        env2.define("y", Value::Int(2));

        // Both see both bindings (same underlying HashMap)
        assert_eq!(env1.get("x"), Some(Value::Int(1)));
        assert_eq!(env1.get("y"), Some(Value::Int(2)));
        assert_eq!(env2.get("x"), Some(Value::Int(1)));
        assert_eq!(env2.get("y"), Some(Value::Int(2)));
    }

    #[test]
    fn test_env_default() {
        let env = Env::default();
        assert!(env.get("anything").is_none());
    }

    #[test]
    fn test_env_stores_different_types() {
        let env = Env::new();
        env.define("int", Value::Int(42));
        env.define("float", Value::Float(2.5));
        env.define("string", Value::String("hello".to_string()));
        env.define("bool", Value::Bool(true));
        env.define("nil", Value::Nil);
        env.define("symbol", Value::Symbol("sym".to_string()));
        env.define("list", Value::List(vec![Value::Int(1), Value::Int(2)]));

        assert_eq!(env.get("int"), Some(Value::Int(42)));
        assert_eq!(env.get("float"), Some(Value::Float(2.5)));
        assert_eq!(env.get("string"), Some(Value::String("hello".to_string())));
        assert_eq!(env.get("bool"), Some(Value::Bool(true)));
        assert_eq!(env.get("nil"), Some(Value::Nil));
        assert_eq!(env.get("symbol"), Some(Value::Symbol("sym".to_string())));
    }

    #[test]
    fn test_env_empty_string_name() {
        let env = Env::new();
        env.define("", Value::Int(42));
        assert_eq!(env.get(""), Some(Value::Int(42)));
    }

    #[test]
    fn test_env_special_char_names() {
        let env = Env::new();
        env.define("null?", Value::Bool(true));
        env.define("string->symbol", Value::Symbol("fn".to_string()));
        env.define("+", Value::Symbol("plus".to_string()));
        env.define("*weird-name*", Value::Int(1));

        assert_eq!(env.get("null?"), Some(Value::Bool(true)));
        assert_eq!(env.get("string->symbol"), Some(Value::Symbol("fn".to_string())));
        assert_eq!(env.get("+"), Some(Value::Symbol("plus".to_string())));
        assert_eq!(env.get("*weird-name*"), Some(Value::Int(1)));
    }
}
