;; Imported from upstream s7test.scm line 35646.
;; Original form:
;; (test (values (values)) #<unspecified>)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (values (values)))))
       (expected (upstream-safe (lambda () #<unspecified>)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35646 actual expected ok?))
