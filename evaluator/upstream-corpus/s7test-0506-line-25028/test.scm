;; Imported from upstream s7test.scm line 25028.
;; Original form:
;; (test (format #f "~W" (dilambda (lambda () 1) (lambda (x) x))) "(dilambda (lambda () 1) (lambda (x) x))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "~W" (dilambda (lambda () 1) (lambda (x) x))))))
       (expected (upstream-safe (lambda () "(dilambda (lambda () 1) (lambda (x) x))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25028 actual expected ok?))
