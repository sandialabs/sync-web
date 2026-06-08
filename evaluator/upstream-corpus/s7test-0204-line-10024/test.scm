;; Imported from upstream s7test.scm line 10024.
;; Original form:
;; (test ((list 1 (list 2 3)) 1 1) 3)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () ((list 1 (list 2 3)) 1 1))))
       (expected (upstream-safe (lambda () 3)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10024 actual expected ok?))
