;; Test game - simple moving square

(define player-x 400)
(define player-y 300)
(define speed 5)

(define (init)
  (println "Game started!"))

(define (update)
  (if (key-down? 'left)  (set! player-x (- player-x speed)))
  (if (key-down? 'right) (set! player-x (+ player-x speed)))
  (if (key-down? 'up)    (set! player-y (- player-y speed)))
  (if (key-down? 'down)  (set! player-y (+ player-y speed))))

(define (draw)
  (clear color-void)
  (draw-rect player-x player-y 32 32 color-gold)
  (draw-text 10 10 "Arrow keys to move, ESC to quit" color-white))
