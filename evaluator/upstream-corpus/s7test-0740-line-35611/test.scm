;; Imported from upstream s7test.scm line 35611.
;; Original form:
;; (test (apply (values + 1 2) '(3)) 6)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (apply (values + 1 2) '(3)))))
       (expected (upstream-safe (lambda () 6)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35611 actual expected ok?))
