;; Imported from upstream s7test.scm line 35632.
;; Original form:
;; (test (apply begin (values (list 1))) 1)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (apply begin (values (list 1))))))
       (expected (upstream-safe (lambda () 1)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35632 actual expected ok?))
