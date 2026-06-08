;; Imported from upstream s7test.scm line 30905.
;; Original form:
;; (test (let () (define (hi) (let ((x 3/2)) (set! x (- x 2)) x)) (hi) (hi)) -1/2)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let () (define (hi) (let ((x 3/2)) (set! x (- x 2)) x)) (hi) (hi)))))
       (expected (upstream-safe (lambda () -1/2)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 30905 actual expected ok?))
