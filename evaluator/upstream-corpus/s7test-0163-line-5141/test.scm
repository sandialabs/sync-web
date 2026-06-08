;; Imported from upstream s7test.scm line 5141.
;; Original form:
;; (test (syntax? '=>) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (syntax? '=>))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5141 actual expected ok?))
