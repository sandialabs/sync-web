;; Imported from upstream s7test.scm line 35647.
;; Original form:
;; (test (values (values 1)) 1)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (values (values 1)))))
       (expected (upstream-safe (lambda () 1)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35647 actual expected ok?))
