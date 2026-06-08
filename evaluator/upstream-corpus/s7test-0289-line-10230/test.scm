;; Imported from upstream s7test.scm line 10230.
;; Original form:
;; (test (list-tail '(1 2) (list-tail '(0 . 1) 1)) '(2))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (list-tail '(1 2) (list-tail '(0 . 1) 1)))))
       (expected (upstream-safe (lambda () '(2))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10230 actual expected ok?))
