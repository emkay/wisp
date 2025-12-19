use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[cfg(not(target_arch = "wasm32"))]
use std::fs;
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;

use macroquad::prelude::*;
use serde::Deserialize;

use crate::env::Env;
#[cfg(not(target_arch = "wasm32"))]
use crate::eval::resolve_path;
use crate::value::{native_fn, Value};

// --- JSON Parsing Structs (for WASM) ---

#[derive(Deserialize)]
struct JsonMap {
    width: u32,
    height: u32,
    tilewidth: u32,
    tileheight: u32,
    layers: Vec<JsonLayer>,
    tilesets: Vec<JsonTileset>,
}

#[derive(Deserialize)]
struct JsonLayer {
    name: String,
    #[serde(rename = "type")]
    layer_type: String,
    #[serde(default)]
    width: u32,
    #[serde(default)]
    height: u32,
    #[serde(default)]
    data: Vec<u32>,
    #[serde(default)]
    objects: Vec<JsonObject>,
}

#[derive(Deserialize)]
struct JsonTileset {
    firstgid: u32,
    #[serde(default)]
    image: String,
    tilewidth: u32,
    tileheight: u32,
    #[serde(default)]
    columns: u32,
    #[serde(default)]
    spacing: u32,
    #[serde(default)]
    margin: u32,
}

#[derive(Deserialize)]
struct JsonObject {
    id: u32,
    #[serde(default)]
    name: String,
    #[serde(rename = "type", default)]
    obj_type: String,
    x: f32,
    y: f32,
    #[serde(default)]
    width: f32,
    #[serde(default)]
    height: f32,
    #[serde(default)]
    gid: Option<u32>,
    #[serde(default)]
    properties: Vec<JsonProperty>,
}

#[derive(Deserialize)]
struct JsonProperty {
    name: String,
    #[serde(rename = "type")]
    prop_type: String,
    value: serde_json::Value,
}

// --- Data Structures ---

#[derive(Clone)]
struct TiledMap {
    width: u32,
    height: u32,
    tile_width: u32,
    tile_height: u32,
    layers: Vec<TileLayer>,
    tilesets: Vec<Option<Tileset>>, // None = image collection tileset (not supported)
    objects: Vec<MapObject>,
}

#[derive(Clone)]
struct TileLayer {
    name: String,
    tiles: Vec<TileData>,
    width: u32,
    height: u32,
}

#[derive(Clone, Copy, Default)]
struct TileData {
    gid: u32,     // 0 = empty
    flip_h: bool, // horizontal flip
    flip_v: bool, // vertical flip
    flip_d: bool, // diagonal flip (rotation)
}

#[derive(Clone)]
struct Tileset {
    first_gid: u32,
    tile_width: u32,
    tile_height: u32,
    columns: u32,
    spacing: u32,
    margin: u32,
    texture_path: String,
}

#[derive(Clone)]
struct MapObject {
    id: u32,
    name: String,
    obj_type: String,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    gid: Option<u32>,
    properties: HashMap<String, Value>,
}

thread_local! {
    static TEXTURES: RefCell<HashMap<String, Texture2D>> = RefCell::new(HashMap::new());
    static MAPS: RefCell<HashMap<String, TiledMap>> = RefCell::new(HashMap::new());
}

/// Loads the [`Env`] and binds some native functions. That means these functions are in scope to
/// use within Wisp.
pub fn load_tiled(env: &Env) {
    env.define("load-map", native_fn(load_map));
    env.define("draw-map", native_fn(draw_map));
    env.define("draw-sprite", native_fn(draw_sprite));
    env.define("tile-at", native_fn(tile_at));
    env.define("tile-walkable?", native_fn(tile_walkable));
    env.define("objects-at", native_fn(objects_at));
    env.define("map-objects", native_fn(map_objects));
    env.define("map-width", native_fn(map_width));
    env.define("map-height", native_fn(map_height));
}

#[cfg(not(target_arch = "wasm32"))]
fn load_texture_sync(path: &str) -> Result<Texture2D, String> {
    let bytes = fs::read(path).map_err(|e| format!("Failed to read texture '{}': {}", path, e))?;
    let texture = Texture2D::from_file_with_format(&bytes, None);
    texture.set_filter(FilterMode::Nearest);
    Ok(texture)
}

#[cfg(not(target_arch = "wasm32"))]
fn ensure_texture_loaded(texture_path: &str) -> Result<(), String> {
    let already_loaded = TEXTURES.with(|t| t.borrow().contains_key(texture_path));
    if !already_loaded {
        let texture = load_texture_sync(texture_path)?;
        TEXTURES.with(|t| t.borrow_mut().insert(texture_path.to_string(), texture));
    }
    Ok(())
}

fn get_texture(path: &str) -> Option<Texture2D> {
    TEXTURES.with(|t| t.borrow().get(path).cloned())
}

#[cfg(not(target_arch = "wasm32"))]
fn extract_tilesets(tiled_map: &tiled::Map, parent_dir: &Path) -> Vec<Option<Tileset>> {
    let mut tilesets = Vec::new();
    let mut first_gid = 1u32;

    for tileset in tiled_map.tilesets() {
        let tile_count = tileset.tilecount;

        if let Some(image) = &tileset.image {
            let texture_path = resolve_texture_path(&image.source, parent_dir);
            let actual_tile_count = if tile_count > 0 {
                tile_count
            } else {
                (image.width as u32 / tileset.tile_width)
                    * (image.height as u32 / tileset.tile_height)
            };

            tilesets.push(Some(Tileset {
                first_gid,
                tile_width: tileset.tile_width,
                tile_height: tileset.tile_height,
                columns: tileset.columns,
                spacing: tileset.spacing,
                margin: tileset.margin,
                texture_path,
            }));

            first_gid += actual_tile_count;
        } else {
            // Image collection tileset - not supported, add placeholder
            tilesets.push(None);
            first_gid += tile_count;
        }
    }

    tilesets
}

#[cfg(not(target_arch = "wasm32"))]
fn resolve_texture_path(source: &Path, parent_dir: &Path) -> String {
    if source.exists() {
        source.to_string_lossy().to_string()
    } else {
        parent_dir.join(source).to_string_lossy().to_string()
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn extract_layers(tiled_map: &tiled::Map, tilesets: &[Option<Tileset>]) -> Vec<TileLayer> {
    let mut layers = Vec::new();

    for layer in tiled_map.layers() {
        if let Some(tile_layer) = layer.as_tile_layer() {
            let name = layer.name.clone();
            let width = tile_layer.width().unwrap_or(tiled_map.width);
            let height = tile_layer.height().unwrap_or(tiled_map.height);
            let tiles = extract_layer_tiles(tile_layer, width, height, tilesets);
            layers.push(TileLayer { name, tiles, width, height });
        }
    }

    layers
}

#[cfg(not(target_arch = "wasm32"))]
fn extract_layer_tiles(
    tile_layer: tiled::TileLayer,
    width: u32,
    height: u32,
    tilesets: &[Option<Tileset>],
) -> Vec<TileData> {
    let mut tiles = Vec::with_capacity((width * height) as usize);

    for y in 0..height {
        for x in 0..width {
            let tile_data = tile_layer
                .get_tile(x as i32, y as i32)
                .map(|t| {
                    let first_gid = tilesets
                        .get(t.tileset_index())
                        .and_then(|opt| opt.as_ref())
                        .map(|ts| ts.first_gid)
                        .unwrap_or(1);

                    TileData {
                        gid: first_gid + t.id(),
                        flip_h: t.flip_h,
                        flip_v: t.flip_v,
                        flip_d: t.flip_d,
                    }
                })
                .unwrap_or_default();
            tiles.push(tile_data);
        }
    }

    tiles
}

#[cfg(not(target_arch = "wasm32"))]
fn extract_objects(tiled_map: &tiled::Map) -> Vec<MapObject> {
    let mut objects = Vec::new();

    for layer in tiled_map.layers() {
        if let Some(obj_layer) = layer.as_object_layer() {
            for obj in obj_layer.objects() {
                objects.push(convert_object(obj));
            }
        }
    }

    objects
}

#[cfg(not(target_arch = "wasm32"))]
fn convert_object(obj: tiled::Object) -> MapObject {
    let properties = obj
        .properties
        .iter()
        .map(|(k, v)| (k.clone(), tiled_property_to_value(v)))
        .collect();

    let (width, height) = object_dimensions(&obj.shape);

    // Get tile ID if this is a tile object
    let gid = obj.tile_data().map(|td| td.id());

    // For tile objects: adjust y (Tiled uses bottom anchor) and snap to grid
    let (x, y) = if gid.is_some() && width > 0.0 && height > 0.0 {
        let adjusted_y = obj.y - height;
        (
            (obj.x / width).round() * width,
            (adjusted_y / height).round() * height,
        )
    } else {
        (obj.x, obj.y)
    };

    MapObject {
        id: obj.id(),
        name: obj.name.clone(),
        obj_type: obj.user_type.clone(),
        x,
        y,
        width,
        height,
        gid,
        properties,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn object_dimensions(shape: &tiled::ObjectShape) -> (f32, f32) {
    match shape {
        tiled::ObjectShape::Rect { width, height } => (*width, *height),
        tiled::ObjectShape::Ellipse { width, height } => (*width, *height),
        tiled::ObjectShape::Text { width, height, .. } => (*width, *height),
        _ => (0.0, 0.0),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn tiled_property_to_value(prop: &tiled::PropertyValue) -> Value {
    match prop {
        tiled::PropertyValue::BoolValue(b) => Value::Bool(*b),
        tiled::PropertyValue::FloatValue(f) => Value::Float(*f as f64),
        tiled::PropertyValue::IntValue(i) => Value::Int(*i as i64),
        tiled::PropertyValue::StringValue(s) => Value::String(s.clone()),
        tiled::PropertyValue::ColorValue(c) => Value::List(vec![
            Value::Symbol("color".to_string()),
            Value::Float(c.red as f64 / 255.0),
            Value::Float(c.green as f64 / 255.0),
            Value::Float(c.blue as f64 / 255.0),
            Value::Float(c.alpha as f64 / 255.0),
        ]),
        _ => Value::Nil,
    }
}

fn find_tileset_for_gid(tilesets: &[Option<Tileset>], gid: u32) -> Option<&Tileset> {
    tilesets
        .iter()
        .rev()
        .filter_map(|opt| opt.as_ref())
        .find(|ts| gid >= ts.first_gid)
}

fn tile_source_rect(tileset: &Tileset, local_id: u32) -> Rect {
    const INSET: f32 = 0.5;

    let col = local_id % tileset.columns;
    let row = local_id / tileset.columns;

    let x = tileset.margin as f32 + col as f32 * (tileset.tile_width + tileset.spacing) as f32;
    let y = tileset.margin as f32 + row as f32 * (tileset.tile_height + tileset.spacing) as f32;

    Rect::new(
        x + INSET,
        y + INSET,
        tileset.tile_width as f32 - INSET * 2.0,
        tileset.tile_height as f32 - INSET * 2.0,
    )
}

/// Convert Tiled flip flags to macroquad (flip_x, flip_y, rotation)
fn tile_flip_transform(flip_d: bool, flip_h: bool, flip_v: bool) -> (bool, bool, f32) {
    match (flip_d, flip_h, flip_v) {
        (false, h, v) => (h, v, 0.0),
        (true, false, false) => (false, false, -std::f32::consts::FRAC_PI_2),
        (true, true, false) => (false, false, std::f32::consts::FRAC_PI_2),
        (true, false, true) => (true, true, std::f32::consts::FRAC_PI_2),
        (true, true, true) => (false, true, -std::f32::consts::FRAC_PI_2),
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_tile(
    texture: &Texture2D,
    source: Rect,
    dest_x: f32,
    dest_y: f32,
    dest_w: f32,
    dest_h: f32,
    flip_x: bool,
    flip_y: bool,
    rotation: f32,
) {
    draw_texture_ex(
        texture,
        dest_x,
        dest_y,
        WHITE,
        DrawTextureParams {
            source: Some(source),
            dest_size: Some(Vec2::new(dest_w, dest_h)),
            flip_x,
            flip_y,
            rotation,
            ..Default::default()
        },
    );
}

/// WASM: Look up preloaded map (must be preloaded before game starts)
#[cfg(target_arch = "wasm32")]
fn load_map(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("load-map requires 1 argument".to_string());
    }
    let path = args[0].as_string("load-map")?;

    // Check if map was preloaded
    if is_map_loaded(&path) {
        Ok(Value::String(path))
    } else {
        Err(format!(
            "load-map: '{}' was not preloaded. In WASM, use JSON format (.json) and ensure the map is referenced in your script.",
            path
        ))
    }
}

/// Native: Load map from filesystem
#[cfg(not(target_arch = "wasm32"))]
fn load_map(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("load-map requires 1 argument".to_string());
    }

    let path_arg = args[0].as_string("load-map")?;
    let resolved = resolve_path(&path_arg);
    let path = resolved.to_string_lossy().to_string();
    let map_path = Path::new(&path);
    let parent_dir = map_path.parent().unwrap_or(Path::new("."));

    let mut loader = tiled::Loader::new();
    let tiled_map = loader
        .load_tmx_map(&path)
        .map_err(|e| format!("load-map: failed to load '{}': {}", path, e))?;

    let tilesets = extract_tilesets(&tiled_map, parent_dir);
    let layers = extract_layers(&tiled_map, &tilesets);
    let objects = extract_objects(&tiled_map);

    // Load all tileset textures
    for tileset in tilesets.iter().flatten() {
        ensure_texture_loaded(&tileset.texture_path)?;
    }

    let map = TiledMap {
        width: tiled_map.width,
        height: tiled_map.height,
        tile_width: tiled_map.tile_width,
        tile_height: tiled_map.tile_height,
        layers,
        tilesets,
        objects,
    };

    MAPS.with(|maps| maps.borrow_mut().insert(path.clone(), map));

    Ok(Value::String(path))
}

fn draw_map(args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() || args.len() > 3 {
        return Err("draw-map requires 1-3 arguments".to_string());
    }

    let map_id = args[0].as_string("draw-map")?;
    let offset_x = if args.len() > 1 { args[1].as_f32("draw-map")? } else { 0.0 };
    let offset_y = if args.len() > 2 { args[2].as_f32("draw-map")? } else { 0.0 };

    MAPS.with(|maps| {
        let maps = maps.borrow();
        let map = maps
            .get(&map_id)
            .ok_or_else(|| format!("draw-map: unknown map '{}'", map_id))?;

        let tile_w = map.tile_width as f32;
        let tile_h = map.tile_height as f32;

        for layer in &map.layers {
            for y in 0..layer.height {
                for x in 0..layer.width {
                    let tile = layer.tiles[(y * layer.width + x) as usize];
                    if tile.gid == 0 {
                        continue;
                    }

                    if let Some(tileset) = find_tileset_for_gid(&map.tilesets, tile.gid)
                        && let Some(texture) = get_texture(&tileset.texture_path) {
                            let local_id = tile.gid - tileset.first_gid;
                            let source = tile_source_rect(tileset, local_id);
                            let (flip_x, flip_y, rotation) =
                                tile_flip_transform(tile.flip_d, tile.flip_h, tile.flip_v);

                            draw_tile(
                                &texture,
                                source,
                                x as f32 * tile_w + offset_x,
                                y as f32 * tile_h + offset_y,
                                tile_w,
                                tile_h,
                                flip_x,
                                flip_y,
                                rotation,
                            );
                        }
                }
            }
        }

        Ok(Value::Nil)
    })
}

fn draw_sprite(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 4 {
        return Err("draw-sprite requires 4 arguments (map-id tile-id x y)".to_string());
    }

    let map_id = args[0].as_string("draw-sprite")?;
    let tile_id = args[1].as_u32("draw-sprite")?;
    let x = args[2].as_f32("draw-sprite")?;
    let y = args[3].as_f32("draw-sprite")?;

    MAPS.with(|maps| {
        let maps = maps.borrow();
        let map = maps
            .get(&map_id)
            .ok_or_else(|| format!("draw-sprite: unknown map '{}'", map_id))?;

        if let Some(tileset) = map.tilesets.iter().find_map(|opt| opt.as_ref())
            && let Some(texture) = get_texture(&tileset.texture_path) {
                let source = tile_source_rect(tileset, tile_id);
                draw_tile(
                    &texture,
                    source,
                    x,
                    y,
                    tileset.tile_width as f32,
                    tileset.tile_height as f32,
                    false,
                    false,
                    0.0,
                );
            }

        Ok(Value::Nil)
    })
}

fn tile_at(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 3 {
        return Err("tile-at requires 3 arguments".to_string());
    }

    let map_id = args[0].as_string("tile-at")?;
    let x = args[1].as_f32("tile-at")? as u32;
    let y = args[2].as_f32("tile-at")? as u32;

    MAPS.with(|maps| {
        let maps = maps.borrow();
        let map = maps
            .get(&map_id)
            .ok_or_else(|| format!("tile-at: unknown map '{}'", map_id))?;

        if let Some(layer) = map.layers.first()
            && x < layer.width && y < layer.height {
                let idx = (y * layer.width + x) as usize;
                return Ok(Value::Int(layer.tiles[idx].gid as i64));
            }

        Ok(Value::Int(0))
    })
}

fn tile_walkable(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 3 {
        return Err("tile-walkable? requires 3 arguments".to_string());
    }

    let map_id = args[0].as_string("tile-walkable?")?;
    let fx = args[1].as_f32("tile-walkable?")?;
    let fy = args[2].as_f32("tile-walkable?")?;
    let x = fx as u32;
    let y = fy as u32;

    MAPS.with(|maps| {
        let maps = maps.borrow();
        let map = maps
            .get(&map_id)
            .ok_or_else(|| format!("tile-walkable?: unknown map '{}'", map_id))?;

        // Find collision layer (case-insensitive)
        let collision_layer = map
            .layers
            .iter()
            .find(|l| l.name.eq_ignore_ascii_case("collision"));

        match collision_layer {
            Some(layer) if x < layer.width && y < layer.height => {
                let idx = (y * layer.width + x) as usize;
                let gid = layer.tiles[idx].gid;
                Ok(Value::Bool(gid == 0))
            }
            Some(_) => Ok(Value::Bool(false)), // Out of bounds = not walkable
            None => Ok(Value::Bool(true)),      // No collision layer = all walkable
        }
    })
}

fn objects_at(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 3 {
        return Err("objects-at requires 3 arguments".to_string());
    }

    let map_id = args[0].as_string("objects-at")?;
    let px = args[1].as_f32("objects-at")?;
    let py = args[2].as_f32("objects-at")?;

    MAPS.with(|maps| {
        let maps = maps.borrow();
        let map = maps
            .get(&map_id)
            .ok_or_else(|| format!("objects-at: unknown map '{}'", map_id))?;

        let result: Vec<Value> = map
            .objects
            .iter()
            .filter(|obj| {
                px >= obj.x && px < obj.x + obj.width && py >= obj.y && py < obj.y + obj.height
            })
            .map(object_to_value)
            .collect();

        Ok(Value::List(result))
    })
}

fn map_objects(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("map-objects requires 1 argument".to_string());
    }

    let map_id = args[0].as_string("map-objects")?;

    MAPS.with(|maps| {
        let maps = maps.borrow();
        let map = maps
            .get(&map_id)
            .ok_or_else(|| format!("map-objects: unknown map '{}'", map_id))?;

        let result: Vec<Value> = map.objects.iter().map(object_to_value).collect();
        Ok(Value::List(result))
    })
}

fn object_to_value(obj: &MapObject) -> Value {
    let mut obj_map = HashMap::new();
    obj_map.insert("id".to_string(), Value::Int(obj.id as i64));
    obj_map.insert("name".to_string(), Value::String(obj.name.clone()));
    obj_map.insert("type".to_string(), Value::String(obj.obj_type.clone()));
    obj_map.insert("x".to_string(), Value::Float(obj.x as f64));
    obj_map.insert("y".to_string(), Value::Float(obj.y as f64));
    obj_map.insert("width".to_string(), Value::Float(obj.width as f64));
    obj_map.insert("height".to_string(), Value::Float(obj.height as f64));

    if let Some(gid) = obj.gid {
        obj_map.insert("gid".to_string(), Value::Int(gid as i64));
    }

    for (key, value) in &obj.properties {
        obj_map.insert(key.clone(), value.clone());
    }

    Value::HashMap(Rc::new(RefCell::new(obj_map)))
}

fn map_width(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("map-width requires 1 argument".to_string());
    }

    let map_id = args[0].as_string("map-width")?;

    MAPS.with(|maps| {
        let maps = maps.borrow();
        let map = maps
            .get(&map_id)
            .ok_or_else(|| format!("map-width: unknown map '{}'", map_id))?;
        Ok(Value::Int(map.width as i64))
    })
}

fn map_height(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("map-height requires 1 argument".to_string());
    }

    let map_id = args[0].as_string("map-height")?;

    MAPS.with(|maps| {
        let maps = maps.borrow();
        let map = maps
            .get(&map_id)
            .ok_or_else(|| format!("map-height: unknown map '{}'", map_id))?;
        Ok(Value::Int(map.height as i64))
    })
}

// --- WASM Preloading (JSON maps) ---

/// Extract map paths from script source (finds all load-map calls)
pub fn extract_map_paths(source: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut chars = source.chars().peekable();

    while let Some(c) = chars.next() {
        // Look for (load-map "
        if c == '(' {
            let rest: String = chars.clone().take(10).collect();
            if rest.starts_with("load-map ") || rest.starts_with("load-map\"") {
                // Skip "load-map"
                for _ in 0..8 { chars.next(); }
                // Skip whitespace
                while chars.peek().map(|c| c.is_whitespace()).unwrap_or(false) {
                    chars.next();
                }
                // Expect quote
                if chars.peek() == Some(&'"') {
                    chars.next(); // consume opening quote
                    let path: String = chars.by_ref().take_while(|c| *c != '"').collect();
                    if !path.is_empty() {
                        paths.push(path);
                    }
                }
            }
        }
    }

    paths
}

/// Preload a JSON map asynchronously (for WASM)
/// Returns the map ID (path) on success
pub async fn preload_map(path: &str, base_dir: &str) -> Result<String, String> {
    // Resolve path relative to base directory
    let full_path = if path.starts_with('/') || path.starts_with("http") || base_dir.is_empty() {
        path.to_string()
    } else {
        format!("{}/{}", base_dir.trim_end_matches('/'), path)
    };

    // Fetch JSON file
    let json_str = load_string(&full_path).await
        .map_err(|e| format!("Failed to load map '{}': {:?}", full_path, e))?;

    // Parse JSON
    let json_map: JsonMap = serde_json::from_str(&json_str)
        .map_err(|e| format!("Failed to parse map '{}': {}", full_path, e))?;

    // Get base directory for texture paths
    let map_base = full_path.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");

    // Convert tilesets and load textures
    let mut tilesets = Vec::new();
    for ts in &json_map.tilesets {
        if ts.image.is_empty() {
            tilesets.push(None); // Image collection tileset not supported
            continue;
        }

        let texture_path = if ts.image.starts_with('/') || ts.image.starts_with("http") || map_base.is_empty() {
            ts.image.clone()
        } else {
            format!("{}/{}", map_base, ts.image)
        };

        // Load texture async
        let texture = load_texture(&texture_path).await
            .map_err(|e| format!("Failed to load texture '{}': {:?}", texture_path, e))?;
        texture.set_filter(FilterMode::Nearest);

        // Store texture
        TEXTURES.with(|t| t.borrow_mut().insert(texture_path.clone(), texture));

        tilesets.push(Some(Tileset {
            first_gid: ts.firstgid,
            tile_width: ts.tilewidth,
            tile_height: ts.tileheight,
            columns: ts.columns,
            spacing: ts.spacing,
            margin: ts.margin,
            texture_path,
        }));
    }

    // Convert layers
    let mut layers = Vec::new();
    let mut objects = Vec::new();

    for layer in &json_map.layers {
        if layer.layer_type == "tilelayer" {
            let tiles = parse_json_tiles(&layer.data);
            layers.push(TileLayer {
                name: layer.name.clone(),
                tiles,
                width: layer.width,
                height: layer.height,
            });
        } else if layer.layer_type == "objectgroup" {
            for obj in &layer.objects {
                objects.push(convert_json_object(obj));
            }
        }
    }

    let map = TiledMap {
        width: json_map.width,
        height: json_map.height,
        tile_width: json_map.tilewidth,
        tile_height: json_map.tileheight,
        layers,
        tilesets,
        objects,
    };

    // Store map with original path as key (so Wisp code can reference it)
    MAPS.with(|maps| maps.borrow_mut().insert(path.to_string(), map));

    Ok(path.to_string())
}

fn parse_json_tiles(data: &[u32]) -> Vec<TileData> {
    const FLIP_H: u32 = 0x80000000;
    const FLIP_V: u32 = 0x40000000;
    const FLIP_D: u32 = 0x20000000;
    const GID_MASK: u32 = 0x1FFFFFFF;

    data.iter().map(|&raw| {
        let gid = raw & GID_MASK;
        TileData {
            gid,
            flip_h: (raw & FLIP_H) != 0,
            flip_v: (raw & FLIP_V) != 0,
            flip_d: (raw & FLIP_D) != 0,
        }
    }).collect()
}

fn convert_json_object(obj: &JsonObject) -> MapObject {
    let mut properties = HashMap::new();
    for prop in &obj.properties {
        let value = match prop.prop_type.as_str() {
            "bool" => Value::Bool(prop.value.as_bool().unwrap_or(false)),
            "int" => Value::Int(prop.value.as_i64().unwrap_or(0)),
            "float" => Value::Float(prop.value.as_f64().unwrap_or(0.0)),
            _ => Value::String(prop.value.as_str().unwrap_or("").to_string()),
        };
        properties.insert(prop.name.clone(), value);
    }

    // For tile objects: adjust y (Tiled uses bottom anchor) and snap to grid
    let (x, y) = if obj.gid.is_some() && obj.width > 0.0 && obj.height > 0.0 {
        let adjusted_y = obj.y - obj.height;
        (
            (obj.x / obj.width).round() * obj.width,
            (adjusted_y / obj.height).round() * obj.height,
        )
    } else {
        (obj.x, obj.y)
    };

    MapObject {
        id: obj.id,
        name: obj.name.clone(),
        obj_type: obj.obj_type.clone(),
        x,
        y,
        width: obj.width,
        height: obj.height,
        gid: obj.gid,
        properties,
    }
}

/// Check if a map is already loaded
pub fn is_map_loaded(path: &str) -> bool {
    MAPS.with(|maps| maps.borrow().contains_key(path))
}
