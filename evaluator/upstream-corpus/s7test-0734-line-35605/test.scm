;; Imported from upstream s7test.scm line 35605.
;; Original form:
;; (test (+ (cond (#t (values 1 2)) (#f (values 3 4))) 5) 8)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (+ (cond (#t (values 1 2)) (#f (values 3 4))) 5))))
       (expected (upstream-safe (lambda () 8)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35605 actual expected ok?))
