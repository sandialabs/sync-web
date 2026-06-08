;; Imported from upstream s7test.scm line 20627.
;; Original form:
;; (test (integer? (port-position (current-input-port))) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (integer? (port-position (current-input-port))))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 20627 actual expected ok?))
