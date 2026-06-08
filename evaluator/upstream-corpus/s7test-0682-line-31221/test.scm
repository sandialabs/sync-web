;; Imported from upstream s7test.scm line 31221.
;; Original form:
;; (test (if (*) 1 2) 1)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (if (*) 1 2))))
       (expected (upstream-safe (lambda () 1)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31221 actual expected ok?))
