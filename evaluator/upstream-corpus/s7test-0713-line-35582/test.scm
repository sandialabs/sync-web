;; Imported from upstream s7test.scm line 35582.
;; Original form:
;; (test (if (values) 1 2) 1)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (if (values) 1 2))))
       (expected (upstream-safe (lambda () 1)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35582 actual expected ok?))
