;; Imported from upstream s7test.scm line 10193.
;; Original form:
;; (test (let ((a 1)) (list a 2)) '(1 2))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((a 1)) (list a 2)))))
       (expected (upstream-safe (lambda () '(1 2))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10193 actual expected ok?))
