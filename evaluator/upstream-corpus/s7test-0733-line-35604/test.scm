;; Imported from upstream s7test.scm line 35604.
;; Original form:
;; (test (+ (cond (#f (values 1 2)) (#t (values 3 4))) 5) 12)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (+ (cond (#f (values 1 2)) (#t (values 3 4))) 5))))
       (expected (upstream-safe (lambda () 12)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35604 actual expected ok?))
