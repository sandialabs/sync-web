;; Imported from upstream s7test.scm line 10212.
;; Original form:
;; (test (list-tail '(1 2 3) 2) '(3))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (list-tail '(1 2 3) 2))))
       (expected (upstream-safe (lambda () '(3))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10212 actual expected ok?))
