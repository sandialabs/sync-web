;; Imported from upstream s7test.scm line 16423.
;; Original form:
;; (test ((lambda () (abs (#_logand 2 3)))) 2)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () ((lambda () (abs (#_logand 2 3)))))))
       (expected (upstream-safe (lambda () 2)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16423 actual expected ok?))
