;; Imported from upstream s7test.scm line 21473.
;; Original form:
;; (test (display '{ #f) '{)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (display '{ #f))))
       (expected (upstream-safe (lambda () '{)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21473 actual expected ok?))
