;; Imported from upstream s7test.scm line 10211.
;; Original form:
;; (test (list-tail '(1 2 3) 0) '(1 2 3))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (list-tail '(1 2 3) 0))))
       (expected (upstream-safe (lambda () '(1 2 3))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10211 actual expected ok?))
