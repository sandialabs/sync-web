;; Imported from upstream s7test.scm line 16600.
;; Original form:
;; (test (hash-code +nan.0) 0)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (hash-code +nan.0))))
       (expected (upstream-safe (lambda () 0)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16600 actual expected ok?))
