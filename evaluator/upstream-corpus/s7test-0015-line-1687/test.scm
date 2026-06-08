;; Imported from upstream s7test.scm line 1687.
;; Original form:
;; (test (eq? (cdr '(a)) ()) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? (cdr '(a)) ()))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1687 actual expected ok?))
