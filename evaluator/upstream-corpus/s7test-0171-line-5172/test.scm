;; Imported from upstream s7test.scm line 5172.
;; Original form:
;; (test (letrec ((a (lambda () (procedure? a)))) (a)) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (letrec ((a (lambda () (procedure? a)))) (a)))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5172 actual expected ok?))
