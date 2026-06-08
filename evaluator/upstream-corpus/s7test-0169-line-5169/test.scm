;; Imported from upstream s7test.scm line 5169.
;; Original form:
;; (test (procedure? '(lambda (x) x)) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (procedure? '(lambda (x) x)))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5169 actual expected ok?))
