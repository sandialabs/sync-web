;; Imported from upstream s7test.scm line 35648.
;; Original form:
;; (test (list (values (values 1 2 3))) '(1 2 3))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (list (values (values 1 2 3))))))
       (expected (upstream-safe (lambda () '(1 2 3))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35648 actual expected ok?))
