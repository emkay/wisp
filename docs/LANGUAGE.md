# Language

If you are familiar with Scheme or Lisp you should feel comfortable with Wisp. The core of the actual language is small.

## Stdlib

### Definitions

`define`

`set!`

`let`

`fn`

`lambda`

### Control Flow

`if`

`cond`

`else`

`do`

`begin`

### Arithmetic

`+`

`-`

`*`

`/`

`mod`

### Comparison

`=`

`<`
`>`
`<=`
`>=`

### Logic

`not`

`and`

`or`


### Lists

`list`

`car`

`cdr`

`cons`

`append`

`length`

`list-ref`

`quote`

`map`

`filter`

### Hash maps

`hash`

`hash-get`

`hash-set!`

`hash-keys`

### Type predicates

`nil?`

`bool?`

`int?`

`float?`

`string?`

`symbol?`

`list?`

`fn?`

`null?`

`hash?`

### Numeric

`floor`

`ceil`

`round`

`int`

`rand`

`noise`

### Strings

`string-append`

`symbol->string`

`string->symbol`

### I/O

`print`

`println`

`load`

### Debugging

`trace-on`

`trace-off`

## Graphics

### Drawing

`clear`

`draw-rect`

`draw-text`

### Colors

`rgb`

`rgba`

`color-white`

`color-black`

### Screen

`screen-width`

`screen-height`

### Timing

`dt`

`delta-time`

### Input

`key-pressed?`

`key-down?`

`key-released?`

## Audio

`load-sound`

`play-sound`

`play-music`

`stop-sound`

`set-volume`

## Tiled Maps

`load-map`

`draw-map`

`draw-sprite`

`map-width`

`map-height`

`map-objects`

`objects-at`

`tile-at`

`tile-walkable?`

