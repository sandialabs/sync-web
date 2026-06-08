;; Imported from upstream s7test.scm line 35629.
;; Original form:
;; (test ((values values) (values 0)) 0)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () ((values values) (values 0)))))
       (expected (upstream-safe (lambda () 0)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35629 actual expected ok?))
