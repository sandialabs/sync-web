;; Imported from upstream s7test.scm line 35583.
;; Original form:
;; (test (if ((lambda () (values #t #f))) 1 2) 1)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (if ((lambda () (values #t #f))) 1 2))))
       (expected (upstream-safe (lambda () 1)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35583 actual expected ok?))
