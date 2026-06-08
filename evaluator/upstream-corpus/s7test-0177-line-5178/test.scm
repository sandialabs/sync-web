;; Imported from upstream s7test.scm line 5178.
;; Original form:
;; (test (procedure? (lambda* ((a 1)) a)) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (procedure? (lambda* ((a 1)) a)))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5178 actual expected ok?))
