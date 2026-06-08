;; Imported from upstream s7test.scm line 35585.
;; Original form:
;; (test (if (values #t 1) (list (values 2 3))) (list 2 3))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (if (values #t 1) (list (values 2 3))))))
       (expected (upstream-safe (lambda () (list 2 3))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35585 actual expected ok?))
