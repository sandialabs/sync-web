;; Imported from upstream s7test.scm line 10029.
;; Original form:
;; (test ((cons 1 2) 0) 1)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () ((cons 1 2) 0))))
       (expected (upstream-safe (lambda () 1)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10029 actual expected ok?))
