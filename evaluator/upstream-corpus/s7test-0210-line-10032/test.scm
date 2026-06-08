;; Imported from upstream s7test.scm line 10032.
;; Original form:
;; (test (((list (list 1 2 3)) 0) 1) 2)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (((list (list 1 2 3)) 0) 1))))
       (expected (upstream-safe (lambda () 2)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10032 actual expected ok?))
