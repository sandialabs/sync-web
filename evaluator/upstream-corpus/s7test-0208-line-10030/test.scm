;; Imported from upstream s7test.scm line 10030.
;; Original form:
;; (test (list-ref (cons 1 2) 0) 1)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (list-ref (cons 1 2) 0))))
       (expected (upstream-safe (lambda () 1)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10030 actual expected ok?))
