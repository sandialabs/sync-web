;; Imported from upstream s7test.scm line 5168.
;; Original form:
;; (test (procedure? (lambda (x) x)) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (procedure? (lambda (x) x)))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5168 actual expected ok?))
