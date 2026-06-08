;; Imported from upstream s7test.scm line 10201.
;; Original form:
;; (test (list 1(list 2)) '(1(2)))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (list 1(list 2)))))
       (expected (upstream-safe (lambda () '(1(2)))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10201 actual expected ok?))
