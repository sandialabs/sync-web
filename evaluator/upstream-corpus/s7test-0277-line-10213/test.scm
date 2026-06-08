;; Imported from upstream s7test.scm line 10213.
;; Original form:
;; (test (list-tail '(1 2 3) 3) ())

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (list-tail '(1 2 3) 3))))
       (expected (upstream-safe (lambda () ())))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10213 actual expected ok?))
