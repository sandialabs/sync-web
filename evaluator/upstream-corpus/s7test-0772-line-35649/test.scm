;; Imported from upstream s7test.scm line 35649.
;; Original form:
;; (test (values (values 'one)) 'one)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (values (values 'one)))))
       (expected (upstream-safe (lambda () 'one)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35649 actual expected ok?))
