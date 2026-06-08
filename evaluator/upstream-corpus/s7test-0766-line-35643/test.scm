;; Imported from upstream s7test.scm line 35643.
;; Original form:
;; (test (begin (values 1 2 3) 4) 4)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (begin (values 1 2 3) 4))))
       (expected (upstream-safe (lambda () 4)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35643 actual expected ok?))
