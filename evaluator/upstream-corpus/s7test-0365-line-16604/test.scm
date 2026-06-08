;; Imported from upstream s7test.scm line 16604.
;; Original form:
;; (test (hash-code 1.0e15) 1000000000000000)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (hash-code 1.0e15))))
       (expected (upstream-safe (lambda () 1000000000000000)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16604 actual expected ok?))
