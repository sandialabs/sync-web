;; Imported from upstream s7test.scm line 35607.
;; Original form:
;; (test (apply + (list ((lambda (n) (values n (+ n 1))) 1))) 3)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (apply + (list ((lambda (n) (values n (+ n 1))) 1))))))
       (expected (upstream-safe (lambda () 3)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35607 actual expected ok?))
