;; Towers of Hanoi simulation with move generation and state validation.

(define (make-initial-pegs n)
  (list (cons 'a (let loop ((i n) (out '()))
                   (if (= i 0) out (loop (- i 1) (cons i out)))))
        (cons 'b '())
        (cons 'c '())))

(define (peg-ref pegs peg)
  (let ((entry (assoc peg pegs)))
    (if entry (cdr entry) (error 'hanoi-error "missing peg: ~S" peg))))

(define (peg-set pegs peg stack)
  (cond ((null? pegs) (error 'hanoi-error "missing peg in set: ~S" peg))
        ((eq? (caar pegs) peg) (cons (cons peg stack) (cdr pegs)))
        (else (cons (car pegs) (peg-set (cdr pegs) peg stack)))))

(define (legal-move? pegs from to)
  (let ((src (peg-ref pegs from))
        (dst (peg-ref pegs to)))
    (and (pair? src)
         (or (null? dst) (< (car src) (car dst))))))

(define (apply-move pegs move)
  (let* ((from (car move))
         (to (cadr move))
         (src (peg-ref pegs from))
         (dst (peg-ref pegs to)))
    (if (not (legal-move? pegs from to))
        (error 'hanoi-error "illegal move: ~S in ~S" move pegs)
        (peg-set (peg-set pegs from (cdr src)) to (cons (car src) dst)))))

(define (hanoi-moves n from to spare)
  (if (= n 0)
      '()
      (append (hanoi-moves (- n 1) from spare to)
              (list (list from to))
              (hanoi-moves (- n 1) spare to from))))

(define (simulate pegs moves)
  (let loop ((state pegs) (rest moves) (states (list pegs)))
    (if (null? rest)
        (list state (reverse states))
        (let ((next (apply-move state (car rest))))
          (loop next (cdr rest) (cons next states))))))

(define (all-legal-prefixes? n moves)
  (let loop ((state (make-initial-pegs n)) (rest moves))
    (if (null? rest)
        #t
        (and (legal-move? state (caar rest) (cadar rest))
             (loop (apply-move state (car rest)) (cdr rest))))))

(define (expected-final n target)
  (peg-set (peg-set (make-initial-pegs n) 'a '()) target
           (let loop ((i n) (out '()))
             (if (= i 0) out (loop (- i 1) (cons i out))))))

(let* ((n 5)
       (moves (hanoi-moves n 'a 'c 'b))
       (result (simulate (make-initial-pegs n) moves))
       (final (car result))
       (states (cadr result)))
  (list
    (list 'move-count (length moves))
    (list 'first-five (let loop ((xs moves) (i 5) (out '()))
                        (if (or (= i 0) (null? xs))
                            (reverse out)
                            (loop (cdr xs) (- i 1) (cons (car xs) out)))))
    (list 'last-five (let loop ((xs (reverse moves)) (i 5) (out '()))
                       (if (or (= i 0) (null? xs))
                           out
                           (loop (cdr xs) (- i 1) (cons (car xs) out)))))
    (list 'legal? (all-legal-prefixes? n moves))
    (list 'final final)
    (list 'final-ok? (equal? final (expected-final n 'c)))
    (list 'state-count (length states))))
