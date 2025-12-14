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

