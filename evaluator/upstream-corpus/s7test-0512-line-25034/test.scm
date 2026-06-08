;; Imported from upstream s7test.scm line 25034.
;; Original form:
;; (test (format #f "~W" (define _definee_ (dilambda (lambda args args) (lambda args args)))) "(dilambda (lambda args args) (lambda args args))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "~W" (define _definee_ (dilambda (lambda args args) (lambda args args)))))))
       (expected (upstream-safe (lambda () "(dilambda (lambda args args) (lambda args args))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25034 actual expected ok?))
