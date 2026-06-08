;; Imported from upstream s7test.scm line 31187.
;; Original form:
;; (test (if (or) 1 2) 2)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (if (or) 1 2))))
       (expected (upstream-safe (lambda () 2)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31187 actual expected ok?))
