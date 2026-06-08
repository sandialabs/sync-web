;; Imported from upstream s7test.scm line 5046.
;; Original form:
;; (test (symbol? (car '(a b))) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (symbol? (car '(a b))))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5046 actual expected ok?))
