# ISSUES.md

Code review findings and potential improvements for Wisp.

---

## Bugs

### Debug output left in production code (tiled.rs)

Multiple `eprintln!` debug statements are scattered throughout:
- Lines 168-171: Tileset loading debug
- Lines 186-189: Collection tileset debug
- Lines 220-226: Tile loading debug
- Lines 397-416: Flip combo tracking with static AtomicU8
- Lines 473-483: Missing tileset warning

These should either be removed or gated behind a debug flag/environment variable.

### tile_at only checks first layer (tiled.rs)

```rust
if let Some(layer) = map.layers.first() {
```

This ignores all layers except the first. Should either:
- Take an optional layer name/index parameter
- Check all layers and return the topmost non-zero tile
- Document that it only checks the first layer

### tile_walkable? is overly simplistic (tiled.rs)

Currently just returns `true` if tile GID is 0 (empty). The comment says "A more sophisticated version would check tile properties" - Tiled supports custom properties on tiles that could indicate walkability.

---

## Code Quality

### Inconsistent argument extraction pattern

Native functions use verbose match statements for argument extraction. For example in `tiled.rs`:

```rust
let map_id = match &args[0] {
    Value::String(s) => s.clone(),
    _ => return Err("draw-map: expected map-id string".to_string()),
};
```

Consider a helper macro or function like:
```rust
fn expect_string(v: &Value, context: &str) -> Result<String, String>
```

### Large functions in tiled.rs

- `load_map`: ~200 lines, does tileset loading, layer extraction, object extraction, and texture loading
- `draw_map`: ~170 lines with deep nesting

Consider breaking into smaller functions for readability.

### Inefficient tileset lookup in draw_map (tiled.rs)

For every tile rendered, the code iterates through all tilesets to find the matching one:
```rust
for tileset_opt in map.tilesets.iter().rev() {
```

For maps with many tiles and multiple tilesets, this is O(tiles * tilesets). Could precompute a GID-to-tileset index mapping during load_map.

---

## Missing Features / Limitations

### No tail call optimization (eval.rs)

Recursive Wisp functions will overflow the Rust stack. For a game scripting language, this may not be critical, but deeply recursive algorithms will fail.

### No error location information (parse.rs)

Parse errors don't include line/column numbers:
```rust
Err("unterminated string".to_string())
```

Would be more helpful as: `"unterminated string at line 5, column 12"`

### REPL doesn't load runtime (main.rs)

```rust
let env = Env::new();
load_stdlib(&env);
// load_runtime not called
```

Graphics functions aren't available in REPL mode. This is intentional (no window), but `load-map` could still be useful for testing map loading.

### HashMap equality always false (value.rs)

The `PartialEq` impl for `Value` doesn't handle `HashMap`:
```rust
_ => false,
```

Two HashMaps with identical contents are never equal.

### No variadic function support

Can't define Wisp functions that accept variable numbers of arguments. Native functions can (they receive `Vec<Value>`), but user-defined functions require exact arity match.

### Integer overflow not handled (stdlib.rs)

Arithmetic operations cast through f64 which can lose precision for large integers, and integer overflow isn't detected:
```rust
Ok(Value::Int(sum as i64))  // Could overflow
```

---

## Performance

### Excessive cloning

Many places clone Values that could potentially use references:
- `env.get()` clones the value (env.rs)
- Most native functions clone their arguments
- `eval` clones self-evaluating values (eval.rs)

For a game loop running 60fps, this creates GC pressure.

### Texture lookup every frame (tiled.rs)

```rust
let texture = TEXTURES.with(|textures| {
    let textures = textures.borrow();
    textures.get(&tileset.texture_path).cloned()
});
```

This HashMap lookup + clone happens for every tile, every frame. Could cache texture references in the TiledMap struct.

### String allocations in hot paths

Error message formatting allocates strings even when not needed:
```rust
.ok_or_else(|| format!("draw-map: unknown map '{}'", map_id))?;
```

---

## Suggestions

### Add a --debug flag for verbose output

Instead of hardcoded eprintln!, use:
```rust
if std::env::var("WISP_DEBUG").is_ok() {
    eprintln!("...");
}
```

### Consider exposing delta_time to Wisp

Currently no way for scripts to do frame-rate-independent movement. macroquad provides `get_frame_time()`.
