;; Imported from upstream s7test.scm line 5182.
;; Original form:
;; (test (procedure? (bignum "1e100")) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (procedure? (bignum "1e100")))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5182 actual expected ok?))
