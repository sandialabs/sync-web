;; Imported from upstream s7test.scm line 35637.
;; Original form:
;; (test (list? (values (list 1 2 3))) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (list? (values (list 1 2 3))))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35637 actual expected ok?))
