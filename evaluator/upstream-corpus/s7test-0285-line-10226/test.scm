;; Imported from upstream s7test.scm line 10226.
;; Original form:
;; (test (list-tail ''foo 1) '(foo))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (list-tail ''foo 1))))
       (expected (upstream-safe (lambda () '(foo))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10226 actual expected ok?))
