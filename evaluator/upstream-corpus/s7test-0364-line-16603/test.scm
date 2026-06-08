;; Imported from upstream s7test.scm line 16603.
;; Original form:
;; (test (hash-code -1.0e17) 0)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (hash-code -1.0e17))))
       (expected (upstream-safe (lambda () 0)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16603 actual expected ok?))
