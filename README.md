# wisp

scheme like programming language geared towards games

## Summary

This is an interpreter built in a Rust for a programming language based on Scheme. The goal is to be able to create small games that have the basics of an engine built in.


## Usage

`wisp game.wisp`

## Example Wisp Program

```scheme
(define (square x)
    (* x x))

(println "square 8: " (square 8))
```

## Language

If you are familiar with Scheme or Lisp you should feel comfortable with Wisp. The core of the actual language is small.

### Stdlib

Definitions: `define` `set!` `let` `fn` `lambda`
Control Flow: `if` `cond` `else` `do` `begin`
Arithmetic: `+` `-` `*` `/` `mod`
Comparison: = < > <= >=
Logic: `not` `and` `or`
Lists: `list` `car` `cdr` `cons` `append` `length` `list-ref` `quote`
Hash maps: `hash` `hash-get` `hash-set!` `hash-keys` hash?
Type predicates: `nil?` `bool?` `int?` `float?` `string?` `symbol?` `list?` `fn?` `null?`
Numeric: `floor` `ceil` `round` `int`
Strings: `string-append` `symbol->string` `string->symbol`
I/O: `print` `println` `load`
Debugging: `trace-on` `trace-off`

### Graphics

Drawing: `clear` `draw-rect` `draw-text`
Colors: `rgb` `rgba` `color-void` `color-gold` `color-stone` `color-white` `color-black`
Screen: `screen-width` `screen-height`
Timing: `dt` `delta-time`

### Input

`key-pressed?` `key-down?` `key-released?`

### Audio

`load-sound` `play-sound` `play-music` `stop-sound` `set-volume`

### Tiled Maps

`load-map` `draw-map` `draw-sprite` `map-width` `map-height` `map-objects` `objects-at` `tile-at` `tile-walkable?`

## Game Library

Wisp is very much a batteries included language. While the core of the language is small, it comes with a game library welded to it. This library is the [Macroquad](https://macroquad.rs/) game library for Rust. It comes with a huge list of features to build 2D games, and Wisp will be taking full advantage of that so that we don't have to implement those. The way this works is that there will be an interface to the libarary exposed so that you will have access to Macroquad features from Wisp.

## Map Library

We also wanted a way to build out maps and use tilesheets. In order to do this we are leaning heavily on the [Tiled map editor](https://www.mapeditor.org/). We are using the [`tiled` crate](https://crates.io/crates/tiled) in order to load maps that have been created with Tiled so that we can allow people to build out their maps using this wonderful tool and use them from within Wisp to build out there game and dip into the wonderful features that Macroquad uses. It is very much the 2nd libarary that is welded onto Wisp.


