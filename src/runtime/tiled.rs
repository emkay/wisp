use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::rc::Rc;

use macroquad::prelude::*;

use crate::env::Env;
use crate::value::{native_fn, Value};

// Load texture synchronously from file
fn load_texture_sync(path: &str) -> Result<Texture2D, String> {
    let bytes = fs::read(path).map_err(|e| format!("Failed to read texture '{}': {}", path, e))?;
    let texture = Texture2D::from_file_with_format(&bytes, None);
    texture.set_filter(FilterMode::Nearest);
    Ok(texture)
}

// Global storage for loaded textures and map data
thread_local! {
    static TEXTURES: RefCell<HashMap<String, Texture2D>> = RefCell::new(HashMap::new());
    static MAPS: RefCell<HashMap<String, TiledMap>> = RefCell::new(HashMap::new());
}

#[derive(Clone)]
struct TiledMap {
    width: u32,
    height: u32,
    tile_width: u32,
    tile_height: u32,
    layers: Vec<TileLayer>,
    tilesets: Vec<Option<Tileset>>,  // None = image collection tileset (not supported)
    objects: Vec<MapObject>,
    properties: HashMap<String, Value>,
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
    gid: u32,       // 0 = empty
    flip_h: bool,   // horizontal flip
    flip_v: bool,   // vertical flip
    flip_d: bool,   // diagonal flip (rotation)
}

#[derive(Clone)]
struct Tileset {
    first_gid: u32,
    tile_width: u32,
    tile_height: u32,
    columns: u32,
    spacing: u32,  // pixels between tiles
    margin: u32,   // pixels around the edge
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
    properties: HashMap<String, Value>,
}

pub fn load_tiled(env: &Env) {
    env.define("load-map", native_fn(load_map));
    env.define("draw-map", native_fn(draw_map));
    env.define("draw-sprite", native_fn(draw_sprite));
    env.define("tile-at", native_fn(tile_at));
    env.define("tile-walkable?", native_fn(tile_walkable));
    env.define("objects-at", native_fn(objects_at));
    env.define("map-width", native_fn(map_width));
    env.define("map-height", native_fn(map_height));
}

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

// (load-map "path.tmx") -> map-id (string)
fn load_map(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("load-map requires 1 argument".to_string());
    }

    let path = match &args[0] {
        Value::String(s) => s.clone(),
        _ => return Err("load-map: expected string path".to_string()),
    };

    let map_path = Path::new(&path);
    let parent_dir = map_path.parent().unwrap_or(Path::new("."));

    // Load the TMX file
    let mut loader = tiled::Loader::new();
    let tiled_map = loader
        .load_tmx_map(&path)
        .map_err(|e| format!("load-map: failed to load '{}': {}", path, e))?;

    // Extract map properties
    let mut properties = HashMap::new();
    for (key, value) in &tiled_map.properties {
        properties.insert(key.clone(), tiled_property_to_value(value));
    }

    // Load tilesets and their textures
    // Important: We must include ALL tilesets to maintain index alignment with tileset_index()
    // Tilesets without images get a placeholder entry
    let mut tilesets: Vec<Option<Tileset>> = Vec::new();
    let mut first_gid = 1u32;
    for tileset in tiled_map.tilesets() {
        let tile_width = tileset.tile_width;
        let tile_height = tileset.tile_height;
        let columns = tileset.columns;
        let spacing = tileset.spacing;
        let margin = tileset.margin;

        // Calculate tile count for ALL tilesets (needed for first_gid tracking)
        let tile_count = tileset.tilecount;

        // Get the image path (if this is a spritesheet tileset)
        if let Some(image) = &tileset.image {
            // The tiled crate gives us a path relative to the TMX file
            // Try the source directly first, then join with parent if needed
            let texture_path = if image.source.exists() {
                image.source.to_string_lossy().to_string()
            } else {
                let joined = parent_dir.join(&image.source);
                joined.to_string_lossy().to_string()
            };

            // Use image dimensions if tilecount is 0
            let actual_tile_count = if tile_count > 0 {
                tile_count
            } else {
                (image.width as u32 / tile_width) * (image.height as u32 / tile_height)
            };

            tilesets.push(Some(Tileset {
                first_gid,
                tile_width,
                tile_height,
                columns,
                spacing,
                margin,
                texture_path,
            }));

            first_gid += actual_tile_count;
        } else {
            // Image collection tileset (no single image) - add placeholder
            tilesets.push(None);
            first_gid += tile_count;
        }
    }

    // Extract tile layers
    let mut layers = Vec::new();
    for layer in tiled_map.layers() {
        if let Some(tile_layer) = layer.as_tile_layer() {
            let width = tile_layer.width().unwrap_or(tiled_map.width);
            let height = tile_layer.height().unwrap_or(tiled_map.height);

            let mut tiles = Vec::with_capacity((width * height) as usize);
            for y in 0..height {
                for x in 0..width {
                    let tile_data = tile_layer
                        .get_tile(x as i32, y as i32)
                        .map(|t| {
                            // Get the correct GID by looking up the tileset's first_gid
                            let tileset_idx = t.tileset_index();
                            let local_id = t.id();
                            let first_gid = tilesets
                                .get(tileset_idx)
                                .and_then(|opt| opt.as_ref())
                                .map(|ts| ts.first_gid)
                                .unwrap_or(1);
                            TileData {
                                gid: first_gid + local_id,
                                flip_h: t.flip_h,
                                flip_v: t.flip_v,
                                flip_d: t.flip_d,
                            }
                        })
                        .unwrap_or_default();
                    tiles.push(tile_data);
                }
            }

            layers.push(TileLayer {
                name: layer.name.clone(),
                tiles,
                width,
                height,
            });
        }
    }

    // Extract objects
    let mut objects = Vec::new();
    for layer in tiled_map.layers() {
        if let Some(obj_layer) = layer.as_object_layer() {
            for obj in obj_layer.objects() {
                let mut obj_props = HashMap::new();
                for (key, value) in &obj.properties {
                    obj_props.insert(key.clone(), tiled_property_to_value(value));
                }

                // Get width/height from shape if available
                let (width, height) = match &obj.shape {
                    tiled::ObjectShape::Rect { width, height } => (*width, *height),
                    tiled::ObjectShape::Ellipse { width, height } => (*width, *height),
                    tiled::ObjectShape::Point(_, _) => (0.0, 0.0),
                    tiled::ObjectShape::Polygon { points: _ } => (0.0, 0.0),
                    tiled::ObjectShape::Polyline { points: _ } => (0.0, 0.0),
                    tiled::ObjectShape::Text { width, height, .. } => (*width, *height),
                };

                objects.push(MapObject {
                    id: obj.id(),
                    name: obj.name.clone(),
                    obj_type: obj.user_type.clone(),
                    x: obj.x,
                    y: obj.y,
                    width,
                    height,
                    properties: obj_props,
                });
            }
        }
    }

    // Load textures synchronously (before moving tilesets into map)
    for tileset in tilesets.iter().flatten() {
        let already_loaded =
            TEXTURES.with(|textures| textures.borrow().contains_key(&tileset.texture_path));
        if !already_loaded {
            let texture = load_texture_sync(&tileset.texture_path)?;
            TEXTURES.with(|textures| {
                textures
                    .borrow_mut()
                    .insert(tileset.texture_path.clone(), texture);
            });
        }
    }

    let map = TiledMap {
        width: tiled_map.width,
        height: tiled_map.height,
        tile_width: tiled_map.tile_width,
        tile_height: tiled_map.tile_height,
        layers,
        tilesets,
        objects,
        properties,
    };

    // Store the map
    let map_id = path.clone();
    MAPS.with(|maps| {
        maps.borrow_mut().insert(map_id.clone(), map);
    });

    Ok(Value::String(map_id))
}

// (draw-map map-id) or (draw-map map-id offset-x offset-y)
fn draw_map(args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() || args.len() > 3 {
        return Err("draw-map requires 1-3 arguments".to_string());
    }

    let map_id = match &args[0] {
        Value::String(s) => s.clone(),
        _ => return Err("draw-map: expected map-id string".to_string()),
    };

    let offset_x = if args.len() > 1 {
        match &args[1] {
            Value::Int(n) => *n as f32,
            Value::Float(n) => *n as f32,
            _ => return Err("draw-map: offset-x must be a number".to_string()),
        }
    } else {
        0.0
    };

    let offset_y = if args.len() > 2 {
        match &args[2] {
            Value::Int(n) => *n as f32,
            Value::Float(n) => *n as f32,
            _ => return Err("draw-map: offset-y must be a number".to_string()),
        }
    } else {
        0.0
    };

    MAPS.with(|maps| {
        let maps = maps.borrow();
        let map = maps
            .get(&map_id)
            .ok_or_else(|| format!("draw-map: unknown map '{}'", map_id))?;

        for layer in &map.layers {
            for y in 0..layer.height {
                for x in 0..layer.width {
                    let idx = (y * layer.width + x) as usize;
                    let tile = layer.tiles[idx];

                    if tile.gid == 0 {
                        continue;
                    }

                    // Find the tileset for this GID (iterate in reverse to find highest first_gid <= tile.gid)
                    for tileset_opt in map.tilesets.iter().rev() {
                        let Some(tileset) = tileset_opt else { continue };
                        if tile.gid >= tileset.first_gid {
                            let local_id = tile.gid - tileset.first_gid;

                            // Load texture if needed
                            let texture = TEXTURES.with(|textures| {
                                let textures = textures.borrow();
                                textures.get(&tileset.texture_path).cloned()
                            });

                            if let Some(texture) = texture {
                                // Small inset to avoid tile bleeding at edges
                                const INSET: f32 = 0.5;

                                // Account for margin and spacing in texture coordinates
                                let col = local_id % tileset.columns;
                                let row = local_id / tileset.columns;
                                let src_x = tileset.margin as f32
                                    + col as f32 * (tileset.tile_width + tileset.spacing) as f32;
                                let src_y = tileset.margin as f32
                                    + row as f32 * (tileset.tile_height + tileset.spacing) as f32;

                                let tile_w = map.tile_width as f32;
                                let tile_h = map.tile_height as f32;
                                let dest_x = x as f32 * tile_w + offset_x;
                                let dest_y = y as f32 * tile_h + offset_y;

                                // Tiled flip flags -> macroquad transformation
                                //
                                // Tiled applies: D (diagonal/anti-diagonal flip), then H, then V
                                // macroquad applies: flip BEFORE rotate
                                //
                                // Key identity: flip_y + rot_90 = rot_90 + flip_x
                                // So to get "rot then flip_x", use "flip_y then rot"
                                let (flip_x, flip_y, rotation) = match (tile.flip_d, tile.flip_h, tile.flip_v) {
                                    // No diagonal: direct mapping
                                    (false, h, v) => (h, v, 0.0),
                                    // D alone = 90° CCW
                                    (true, false, false) => (false, false, -std::f32::consts::FRAC_PI_2),
                                    // D+H = 90° CW
                                    (true, true, false) => (false, false, std::f32::consts::FRAC_PI_2),
                                    // D+V: add vertical flip
                                    (true, false, true) => (true, true, std::f32::consts::FRAC_PI_2),
                                    // D+H+V = 90° CCW then flip_x = flip_y then 90° CCW
                                    (true, true, true) => (false, true, -std::f32::consts::FRAC_PI_2),
                                };

                                draw_texture_ex(
                                    &texture,
                                    dest_x,
                                    dest_y,
                                    WHITE,
                                    DrawTextureParams {
                                        source: Some(Rect::new(
                                            src_x + INSET,
                                            src_y + INSET,
                                            tileset.tile_width as f32 - INSET * 2.0,
                                            tileset.tile_height as f32 - INSET * 2.0,
                                        )),
                                        dest_size: Some(macroquad::prelude::Vec2::new(tile_w, tile_h)),
                                        flip_x,
                                        flip_y,
                                        rotation,
                                        ..Default::default()
                                    },
                                );
                            } else {
                                // Draw placeholder rectangle if texture not loaded
                                draw_rectangle(
                                    x as f32 * map.tile_width as f32 + offset_x,
                                    y as f32 * map.tile_height as f32 + offset_y,
                                    map.tile_width as f32,
                                    map.tile_height as f32,
                                    Color::new(0.3, 0.3, 0.3, 1.0),
                                );
                            }
                            break;
                        }
                    }
                }
            }
        }

        Ok(Value::Nil)
    })
}

// (draw-sprite map-id tile-id x y) - draw a single tile at pixel position
fn draw_sprite(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 4 {
        return Err("draw-sprite requires 4 arguments (map-id tile-id x y)".to_string());
    }

    let map_id = match &args[0] {
        Value::String(s) => s.clone(),
        _ => return Err("draw-sprite: expected map-id string".to_string()),
    };

    let tile_id = match &args[1] {
        Value::Int(n) => *n as u32,
        _ => return Err("draw-sprite: tile-id must be an integer".to_string()),
    };

    let x = match &args[2] {
        Value::Int(n) => *n as f32,
        Value::Float(n) => *n as f32,
        _ => return Err("draw-sprite: x must be a number".to_string()),
    };

    let y = match &args[3] {
        Value::Int(n) => *n as f32,
        Value::Float(n) => *n as f32,
        _ => return Err("draw-sprite: y must be a number".to_string()),
    };

    MAPS.with(|maps| {
        let maps = maps.borrow();
        let map = maps
            .get(&map_id)
            .ok_or_else(|| format!("draw-sprite: unknown map '{}'", map_id))?;

        // Use first tileset with an image; tile_id is a local index (0-based)
        if let Some(tileset) = map.tilesets.iter().find_map(|opt| opt.as_ref()) {
            let texture = TEXTURES.with(|textures| {
                textures.borrow().get(&tileset.texture_path).cloned()
            });

            if let Some(texture) = texture {
                const INSET: f32 = 0.5;

                // Account for margin and spacing in texture coordinates
                let col = tile_id % tileset.columns;
                let row = tile_id / tileset.columns;
                let src_x = tileset.margin as f32
                    + col as f32 * (tileset.tile_width + tileset.spacing) as f32;
                let src_y = tileset.margin as f32
                    + row as f32 * (tileset.tile_height + tileset.spacing) as f32;

                draw_texture_ex(
                    &texture,
                    x,
                    y,
                    WHITE,
                    DrawTextureParams {
                        source: Some(Rect::new(
                            src_x + INSET,
                            src_y + INSET,
                            tileset.tile_width as f32 - INSET * 2.0,
                            tileset.tile_height as f32 - INSET * 2.0,
                        )),
                        dest_size: Some(macroquad::prelude::Vec2::new(
                            tileset.tile_width as f32,
                            tileset.tile_height as f32,
                        )),
                        ..Default::default()
                    },
                );
            }
        }

        Ok(Value::Nil)
    })
}

// (tile-at map-id x y) -> tile-gid or 0
fn tile_at(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 3 {
        return Err("tile-at requires 3 arguments".to_string());
    }

    let map_id = match &args[0] {
        Value::String(s) => s.clone(),
        _ => return Err("tile-at: expected map-id string".to_string()),
    };

    let x = match &args[1] {
        Value::Int(n) => *n as u32,
        _ => return Err("tile-at: x must be an integer".to_string()),
    };

    let y = match &args[2] {
        Value::Int(n) => *n as u32,
        _ => return Err("tile-at: y must be an integer".to_string()),
    };

    MAPS.with(|maps| {
        let maps = maps.borrow();
        let map = maps
            .get(&map_id)
            .ok_or_else(|| format!("tile-at: unknown map '{}'", map_id))?;

        // Get tile from first layer (or could be parameterized)
        if let Some(layer) = map.layers.first()
            && x < layer.width && y < layer.height {
                let idx = (y * layer.width + x) as usize;
                return Ok(Value::Int(layer.tiles[idx].gid as i64));
            }

        Ok(Value::Int(0))
    })
}

// (tile-walkable? map-id x y) -> bool
fn tile_walkable(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 3 {
        return Err("tile-walkable? requires 3 arguments".to_string());
    }

    // For now, just check if tile is 0 (empty = walkable) or non-zero
    // A more sophisticated version would check tile properties
    let tile = tile_at(args)?;
    match tile {
        Value::Int(0) => Ok(Value::Bool(true)),
        Value::Int(_) => Ok(Value::Bool(false)), // Non-empty tiles are not walkable by default
        _ => Ok(Value::Bool(false)),
    }
}

// (objects-at map-id x y) -> list of objects
fn objects_at(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 3 {
        return Err("objects-at requires 3 arguments".to_string());
    }

    let map_id = match &args[0] {
        Value::String(s) => s.clone(),
        _ => return Err("objects-at: expected map-id string".to_string()),
    };

    let px = match &args[1] {
        Value::Int(n) => *n as f32,
        Value::Float(n) => *n as f32,
        _ => return Err("objects-at: x must be a number".to_string()),
    };

    let py = match &args[2] {
        Value::Int(n) => *n as f32,
        Value::Float(n) => *n as f32,
        _ => return Err("objects-at: y must be a number".to_string()),
    };

    MAPS.with(|maps| {
        let maps = maps.borrow();
        let map = maps
            .get(&map_id)
            .ok_or_else(|| format!("objects-at: unknown map '{}'", map_id))?;

        let mut result = Vec::new();

        for obj in &map.objects {
            // Check if point is within object bounds
            if px >= obj.x
                && px < obj.x + obj.width
                && py >= obj.y
                && py < obj.y + obj.height
            {
                let mut obj_map = HashMap::new();
                obj_map.insert("id".to_string(), Value::Int(obj.id as i64));
                obj_map.insert("name".to_string(), Value::String(obj.name.clone()));
                obj_map.insert("type".to_string(), Value::String(obj.obj_type.clone()));
                obj_map.insert("x".to_string(), Value::Float(obj.x as f64));
                obj_map.insert("y".to_string(), Value::Float(obj.y as f64));
                obj_map.insert("width".to_string(), Value::Float(obj.width as f64));
                obj_map.insert("height".to_string(), Value::Float(obj.height as f64));

                // Include custom properties
                for (key, value) in &obj.properties {
                    obj_map.insert(key.clone(), value.clone());
                }

                result.push(Value::HashMap(Rc::new(RefCell::new(obj_map))));
            }
        }

        Ok(Value::List(result))
    })
}

// (map-width map-id) -> int
fn map_width(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("map-width requires 1 argument".to_string());
    }

    let map_id = match &args[0] {
        Value::String(s) => s.clone(),
        _ => return Err("map-width: expected map-id string".to_string()),
    };

    MAPS.with(|maps| {
        let maps = maps.borrow();
        let map = maps
            .get(&map_id)
            .ok_or_else(|| format!("map-width: unknown map '{}'", map_id))?;
        Ok(Value::Int(map.width as i64))
    })
}

// (map-height map-id) -> int
fn map_height(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("map-height requires 1 argument".to_string());
    }

    let map_id = match &args[0] {
        Value::String(s) => s.clone(),
        _ => return Err("map-height: expected map-id string".to_string()),
    };

    MAPS.with(|maps| {
        let maps = maps.borrow();
        let map = maps
            .get(&map_id)
            .ok_or_else(|| format!("map-height: unknown map '{}'", map_id))?;
        Ok(Value::Int(map.height as i64))
    })
}

