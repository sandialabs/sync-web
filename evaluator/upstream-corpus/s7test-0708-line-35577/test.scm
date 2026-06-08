;; Imported from upstream s7test.scm line 35577.
;; Original form:
;; (test (if (values '#t) 1 2) 1)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (if (values '#t) 1 2))))
       (expected (upstream-safe (lambda () 1)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35577 actual expected ok?))
