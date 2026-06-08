;; Imported from upstream s7test.scm line 5181.
;; Original form:
;; (test (procedure? (dilambda (lambda () 1) (lambda (x) x))) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (procedure? (dilambda (lambda () 1) (lambda (x) x))))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5181 actual expected ok?))
