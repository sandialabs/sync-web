;; Imported from upstream s7test.scm line 31182.
;; Original form:
;; (test (or '#f ()) ())

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (or '#f ()))))
       (expected (upstream-safe (lambda () ())))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31182 actual expected ok?))
