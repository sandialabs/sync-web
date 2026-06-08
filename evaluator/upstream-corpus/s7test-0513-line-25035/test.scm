;; Imported from upstream s7test.scm line 25035.
;; Original form:
;; (test (format #f "~W" (let () (define (func) (define _definee_ (dilambda (lambda args args) (lambda c c)))) (func))) "(dilambda (lambda args args) (lambda c c))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "~W" (let () (define (func) (define _definee_ (dilambda (lambda args args) (lambda c c)))) (func))))))
       (expected (upstream-safe (lambda () "(dilambda (lambda args args) (lambda c c))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25035 actual expected ok?))
