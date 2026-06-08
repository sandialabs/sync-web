;; Imported from upstream s7test.scm line 16433.
;; Original form:
;; (test (let () (define (hi) (let ((x 0) (i 3)) (do ((i i (+ i 1))) ((= i 6)) (do ((i i (+ i 1))) ((= i 7)) (set! x (+ x i)))) x)) (hi)) 44)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let () (define (hi) (let ((x 0) (i 3)) (do ((i i (+ i 1))) ((= i 6)) (do ((i i (+ i 1))) ((= i 7)) (set! x (+ x i)))) x)) (hi)))))
       (expected (upstream-safe (lambda () 44)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16433 actual expected ok?))
