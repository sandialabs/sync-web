;; Imported from upstream s7test.scm line 10197.
;; Original form:
;; (test (let ((a (list 1 2))) a) '(1 2))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((a (list 1 2))) a))))
       (expected (upstream-safe (lambda () '(1 2))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10197 actual expected ok?))
