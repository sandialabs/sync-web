;; Imported from upstream s7test.scm line 5171.
;; Original form:
;; (test (let ((a (lambda (x) x)))	(procedure? a)) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((a (lambda (x) x)))	(procedure? a)))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5171 actual expected ok?))
