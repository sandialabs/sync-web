;; Imported from upstream s7test.scm line 10215.
;; Original form:
;; (test (list-tail '(1 2 3 . 4) 3) 4)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (list-tail '(1 2 3 . 4) 3))))
       (expected (upstream-safe (lambda () 4)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10215 actual expected ok?))
