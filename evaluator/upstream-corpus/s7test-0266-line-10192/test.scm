;; Imported from upstream s7test.scm line 10192.
;; Original form:
;; (test (list 1) '(1))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (list 1))))
       (expected (upstream-safe (lambda () '(1))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10192 actual expected ok?))
