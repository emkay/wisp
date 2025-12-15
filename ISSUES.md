# ISSUES.md

Code review findings and potential improvements for Wisp.

---

## Bugs

### 1. Debug output left in production code (tiled.rs)

Multiple `eprintln!` debug statements are scattered throughout:
- Lines 168-171: Tileset loading debug
- Lines 186-189: Collection tileset debug
- Lines 220-226: Tile loading debug
- Lines 397-416: Flip combo tracking with static AtomicU8
- Lines 473-483: Missing tileset warning

These should either be removed or gated behind a debug flag/environment variable.

### 2. Dead code warnings (tiled.rs)

Compiler warnings indicate unused fields:
- `TiledMap::properties` (line 35) - extracted but never exposed to Wisp
- `TileLayer::name` (line 40) - stored but never used

Either expose these to Wisp scripts or remove them.

### 3. tile_at only checks first layer (tiled.rs:598)

```rust
if let Some(layer) = map.layers.first() {
```

This ignores all layers except the first. Should either:
- Take an optional layer name/index parameter
- Check all layers and return the topmost non-zero tile
- Document that it only checks the first layer

### 4. tile_walkable? is overly simplistic (tiled.rs:609-623)

Currently just returns `true` if tile GID is 0 (empty). The comment says "A more sophisticated version would check tile properties" - Tiled supports custom properties on tiles that could indicate walkability.

---

## Code Quality

### 5. Duplicated native_fn helper

The same helper function is defined in 4 places:
- `stdlib.rs:54`
- `graphics.rs:26`
- `input.rs:14`
- `tiled.rs:88`

Should be defined once in a shared location (e.g., `value.rs` or a new `util.rs`).

### 6. Inconsistent argument extraction pattern

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

### 7. Large functions in tiled.rs

- `load_map` (lines 110-317): ~200 lines, does tileset loading, layer extraction, object extraction, and texture loading
- `draw_map` (lines 320-491): ~170 lines with deep nesting

Consider breaking into smaller functions for readability.

### 8. Inefficient tileset lookup in draw_map (tiled.rs:368-472)

For every tile rendered, the code iterates through all tilesets to find the matching one:
```rust
for tileset_opt in map.tilesets.iter().rev() {
```

For maps with many tiles and multiple tilesets, this is O(tiles * tilesets). Could precompute a GID-to-tileset index mapping during load_map.

---

## Missing Features / Limitations

### 9. No tail call optimization (eval.rs)

Recursive Wisp functions will overflow the Rust stack. For a game scripting language, this may not be critical, but deeply recursive algorithms will fail.

### 10. No error location information (parse.rs)

Parse errors don't include line/column numbers:
```rust
Err("unterminated string".to_string())
```

Would be more helpful as: `"unterminated string at line 5, column 12"`

### 11. REPL doesn't load runtime (main.rs:108-109)

```rust
let env = Env::new();
load_stdlib(&env);
// load_runtime not called
```

Graphics functions aren't available in REPL mode. This is intentional (no window), but `load-map` could still be useful for testing map loading.

### 12. HashMap equality always false (value.rs:97-112)

The `PartialEq` impl for `Value` doesn't handle `HashMap`:
```rust
_ => false,
```

Two HashMaps with identical contents are never equal.

### 13. No variadic function support

Can't define Wisp functions that accept variable numbers of arguments. Native functions can (they receive `Vec<Value>`), but user-defined functions require exact arity match.

### 14. Integer overflow not handled (stdlib.rs)

Arithmetic operations cast through f64 which can lose precision for large integers, and integer overflow isn't detected:
```rust
Ok(Value::Int(sum as i64))  // Could overflow
```

---

## Performance

### 15. Excessive cloning

Many places clone Values that could potentially use references:
- `env.get()` clones the value (env.rs:39)
- Most native functions clone their arguments
- `eval` clones self-evaluating values (eval.rs:17)

For a game loop running 60fps, this creates GC pressure.

### 16. Texture lookup every frame (tiled.rs:375-378)

```rust
let texture = TEXTURES.with(|textures| {
    let textures = textures.borrow();
    textures.get(&tileset.texture_path).cloned()
});
```

This HashMap lookup + clone happens for every tile, every frame. Could cache texture references in the TiledMap struct.

### 17. String allocations in hot paths

Error message formatting allocates strings even when not needed:
```rust
.ok_or_else(|| format!("draw-map: unknown map '{}'", map_id))?;
```

---

## Suggestions

### 18. Add a --debug flag for verbose output

Instead of hardcoded eprintln!, use:
```rust
if std::env::var("WISP_DEBUG").is_ok() {
    eprintln!("...");
}
```

### 19. Consider exposing delta_time to Wisp

Currently no way for scripts to do frame-rate-independent movement. macroquad provides `get_frame_time()`.
