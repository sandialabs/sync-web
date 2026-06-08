;; Imported from upstream s7test.scm line 16422.
;; Original form:
;; (test ((lambda () (abs (#_logand)))) 1)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () ((lambda () (abs (#_logand)))))))
       (expected (upstream-safe (lambda () 1)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16422 actual expected ok?))
