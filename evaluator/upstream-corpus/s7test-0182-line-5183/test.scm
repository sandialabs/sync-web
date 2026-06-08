;; Imported from upstream s7test.scm line 5183.
;; Original form:
;; (test (procedure? quasiquote) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (procedure? quasiquote))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5183 actual expected ok?))
