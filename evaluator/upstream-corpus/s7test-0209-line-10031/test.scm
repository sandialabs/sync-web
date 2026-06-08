;; Imported from upstream s7test.scm line 10031.
;; Original form:
;; (test (((list (list 1 2 3)) 0) 0) 1)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (((list (list 1 2 3)) 0) 0))))
       (expected (upstream-safe (lambda () 1)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10031 actual expected ok?))
