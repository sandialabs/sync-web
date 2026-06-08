;; Imported from upstream s7test.scm line 35595.
;; Original form:
;; (test (list (values 1 2) (values 3) 4) '(1 2 3 4))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (list (values 1 2) (values 3) 4))))
       (expected (upstream-safe (lambda () '(1 2 3 4))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35595 actual expected ok?))
