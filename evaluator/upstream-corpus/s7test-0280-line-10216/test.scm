;; Imported from upstream s7test.scm line 10216.
;; Original form:
;; (test (let ((x (list 1 2 3))) (eq? (list-tail x 2) (cddr x))) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((x (list 1 2 3))) (eq? (list-tail x 2) (cddr x))))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10216 actual expected ok?))
