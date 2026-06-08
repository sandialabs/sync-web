;; Imported from upstream s7test.scm line 16420.
;; Original form:
;; (test ((lambda () (#_cond (* 1)))) 1)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () ((lambda () (#_cond (* 1)))))))
       (expected (upstream-safe (lambda () 1)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16420 actual expected ok?))
