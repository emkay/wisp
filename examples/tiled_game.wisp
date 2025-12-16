;; Example Tiled-based game
;; Requires: maps/test.tmx and associated tilesheet PNG

(define room nil)
(define player-x 0)
(define player-y 0)
(define player-sprite 0)
(define tile-size 16)
(define music nil)

;; Find an object with a given property value
(define (find-object-by-property objects key value)
  (if (null? objects)
      nil
      (if (= (hash-get (car objects) key) value)
          (car objects)
          (find-object-by-property (cdr objects) key value))))

(define (init)
  ;; Load and play background music
  (set! music (load-sound "audio/sad-town.ogg"))
  (play-music music)

  ;; Load the map - this also loads the tilesheet texture
  (set! room (load-map "maps/test.tmx"))
  (println "Map loaded:" room)
  (println "Map size:" (map-width room) "x" (map-height room))

  ;; Find player spawn from objects
  (let ((objects (map-objects room)))
    (println "Objects:" objects)
    (let ((player-spawn (find-object-by-property objects "type" "player")))
      (if player-spawn
          (do
            (set! player-x (/ (hash-get player-spawn "x") tile-size))
            (set! player-y (/ (hash-get player-spawn "y") tile-size))
            (set! player-sprite (hash-get player-spawn "gid" 0))
            (println "Player spawn:" player-x player-y "sprite:" player-sprite))
          (println "No player spawn found, using default position")))))

(define (update)
  ;; Grid-based movement
  (define new-x player-x)
  (define new-y player-y)

  (if (key-pressed? 'left)  (set! new-x (- player-x 1)))
  (if (key-pressed? 'right) (set! new-x (+ player-x 1)))
  (if (key-pressed? 'up)    (set! new-y (- player-y 1)))
  (if (key-pressed? 'down)  (set! new-y (+ player-y 1)))

  ;; Only move if tile is walkable
  (if (tile-walkable? room new-x new-y)
      (do
        (set! player-x new-x)
        (set! player-y new-y))))

(define (draw)
  (clear color-void)

  ;; Draw the map
  (draw-map room)

  ;; Draw player sprite from tile object
  (draw-sprite room player-sprite (* player-x tile-size) (* player-y tile-size))

  ;; UI
  (draw-text 10 400 "Arrow keys to move" color-white))
