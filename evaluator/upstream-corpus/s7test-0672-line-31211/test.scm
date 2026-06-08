;; Imported from upstream s7test.scm line 31211.
;; Original form:
;; (test (and 1 2 'c '(f g)) '(f g))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (and 1 2 'c '(f g)))))
       (expected (upstream-safe (lambda () '(f g))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31211 actual expected ok?))
