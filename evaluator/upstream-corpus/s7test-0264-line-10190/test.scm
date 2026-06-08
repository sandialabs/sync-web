;; Imported from upstream s7test.scm line 10190.
;; Original form:
;; (test (let ((lst (list 1 (list 2 3)))) lst) '(1 (2 3)))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((lst (list 1 (list 2 3)))) lst))))
       (expected (upstream-safe (lambda () '(1 (2 3)))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10190 actual expected ok?))
