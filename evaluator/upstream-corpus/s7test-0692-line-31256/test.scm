;; Imported from upstream s7test.scm line 31256.
;; Original form:
;; (test (and . (1 2)) 2)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (and . (1 2)))))
       (expected (upstream-safe (lambda () 2)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31256 actual expected ok?))
