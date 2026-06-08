;; Imported from upstream s7test.scm line 10222.
;; Original form:
;; (test (list-tail (cons 1 2) 0) '(1 . 2))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (list-tail (cons 1 2) 0))))
       (expected (upstream-safe (lambda () '(1 . 2))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10222 actual expected ok?))
