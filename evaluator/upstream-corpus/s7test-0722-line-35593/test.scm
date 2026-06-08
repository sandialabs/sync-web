;; Imported from upstream s7test.scm line 35593.
;; Original form:
;; (test (* (values 2 (values 3 4))) 24)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (* (values 2 (values 3 4))))))
       (expected (upstream-safe (lambda () 24)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35593 actual expected ok?))
