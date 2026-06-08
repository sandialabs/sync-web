;; Imported from upstream s7test.scm line 25029.
;; Original form:
;; (test (format #f "~W" (dilambda (lambda (a . b) 1) (lambda (x) x))) "(dilambda (lambda (a . b) 1) (lambda (x) x))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "~W" (dilambda (lambda (a . b) 1) (lambda (x) x))))))
       (expected (upstream-safe (lambda () "(dilambda (lambda (a . b) 1) (lambda (x) x))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25029 actual expected ok?))
