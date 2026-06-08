;; Conway's Game of Life on a bounded grid represented as alists/lists.

(define width 8)
(define height 6)

(define (cell x y) (list x y))

(define (member-cell? c cells)
  (cond ((null? cells) #f)
        ((equal? c (car cells)) #t)
        (else (member-cell? c (cdr cells)))))

(define (alive? world x y)
  (member-cell? (cell x y) world))

(define (in-bounds? x y)
  (and (>= x 0) (< x width) (>= y 0) (< y height)))

(define neighbor-deltas
  '((-1 -1) (0 -1) (1 -1)
    (-1  0)        (1  0)
    (-1  1) (0  1) (1  1)))

(define (live-neighbor-count world x y)
  (let loop ((deltas neighbor-deltas) (count 0))
    (if (null? deltas)
        count
        (let* ((dx (caar deltas))
               (dy (cadar deltas))
               (nx (+ x dx))
               (ny (+ y dy)))
          (loop (cdr deltas)
                (if (and (in-bounds? nx ny) (alive? world nx ny))
                    (+ count 1)
                    count))))))

(define (survives? world x y)
  (let ((n (live-neighbor-count world x y)))
    (if (alive? world x y)
        (or (= n 2) (= n 3))
        (= n 3))))

(define (next-generation world)
  (let y-loop ((y 0) (out '()))
    (if (= y height)
        (reverse out)
        (let x-loop ((x 0) (row-out out))
          (if (= x width)
              (y-loop (+ y 1) row-out)
              (x-loop (+ x 1)
                      (if (survives? world x y)
                          (cons (cell x y) row-out)
                          row-out)))))))

(define (render world)
  (let y-loop ((y 0) (rows '()))
    (if (= y height)
        (reverse rows)
        (let x-loop ((x 0) (chars '()))
          (if (= x width)
              (y-loop (+ y 1) (cons (apply string (reverse chars)) rows))
              (x-loop (+ x 1)
                      (cons (if (alive? world x y) #\# #\.) chars)))))))

(define (population world) (length world))

(define (bounding-box world)
  (if (null? world)
      'empty
      (let loop ((rest (cdr world))
                 (min-x (caar world))
                 (max-x (caar world))
                 (min-y (cadar world))
                 (max-y (cadar world)))
        (if (null? rest)
            (list min-x min-y max-x max-y)
            (let ((x (caar rest))
                  (y (cadar rest)))
              (loop (cdr rest)
                    (min min-x x)
                    (max max-x x)
                    (min min-y y)
                    (max max-y y)))))))

(define (run world steps)
  (let loop ((i 0) (current world) (summary '()))
    (if (> i steps)
        (reverse summary)
        (loop (+ i 1)
              (next-generation current)
              (cons (list 'generation i
                          'population (population current)
                          'box (bounding-box current)
                          'render (render current))
                    summary)))))

;; A glider in the upper-left portion of the bounded board.
(define glider
  (list (cell 1 0)
        (cell 2 1)
        (cell 0 2)
        (cell 1 2)
        (cell 2 2)))

(run glider 4)
