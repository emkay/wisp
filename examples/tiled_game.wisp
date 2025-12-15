;; Example Tiled-based game
;; Requires: maps/test.tmx and associated tilesheet PNG

(define room nil)
(define player-x 5)
(define player-y 5)

(define (init)
  ;; Load the map - this also loads the tilesheet texture
  (set! room (load-map "maps/test.tmx"))
  (println "Map loaded:" room)
  (println "Map size:" (map-width room) "x" (map-height room)))

(define (update)
  ;; Grid-based movement
  (define new-x player-x)
  (define new-y player-y)

  (if (key-pressed? 'left)  (set! new-x (- player-x 1)))
  (if (key-pressed? 'right) (set! new-x (+ player-x 1)))
  (if (key-pressed? 'up)    (set! new-y (- player-y 1)))
  (if (key-pressed? 'down)  (set! new-y (+ player-y 1)))

  ;; Only move if tile is walkable (empty)
  (if (tile-walkable? room new-x new-y)
      (do
        (set! player-x new-x)
        (set! player-y new-y))))

(define (draw)
  (clear color-void)

  ;; Draw the map
  (draw-map room)

  ;; Draw player sprite (tile ID 0 from the tilesheet)
  ;; Position is in pixels: tile coords * 16 (assuming 16x16 tiles)
  (draw-sprite room 0 (* player-x 16) (* player-y 16))

  ;; UI
  (draw-text 10 400 "Arrow keys to move" color-white))
