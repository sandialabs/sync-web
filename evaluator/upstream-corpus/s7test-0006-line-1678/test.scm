;; Imported from upstream s7test.scm line 1678.
;; Original form:
;; (test (eq? "()" ()) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? "()" ()))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1678 actual expected ok?))
