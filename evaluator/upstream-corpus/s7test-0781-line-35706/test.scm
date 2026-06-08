;; Imported from upstream s7test.scm line 35706.
;; Original form:
;; (test (+ 1 ((lambda () ((lambda () (values 2 3)))))) 6)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (+ 1 ((lambda () ((lambda () (values 2 3)))))))))
       (expected (upstream-safe (lambda () 6)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35706 actual expected ok?))
