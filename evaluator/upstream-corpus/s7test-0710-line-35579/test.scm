;; Imported from upstream s7test.scm line 35579.
;; Original form:
;; (test (if (values #f #f) 1 2) 2)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (if (values #f #f) 1 2))))
       (expected (upstream-safe (lambda () 2)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35579 actual expected ok?))
