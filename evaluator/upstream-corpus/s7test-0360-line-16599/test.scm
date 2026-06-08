;; Imported from upstream s7test.scm line 16599.
;; Original form:
;; (test (hash-code (cosh 128)) 0)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (hash-code (cosh 128)))))
       (expected (upstream-safe (lambda () 0)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16599 actual expected ok?))
