;; Imported from upstream s7test.scm line 10034.
;; Original form:
;; (test (let ((lst (list (list 1 2 3)))) (lst 0 1)) 2)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((lst (list (list 1 2 3)))) (lst 0 1)))))
       (expected (upstream-safe (lambda () 2)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10034 actual expected ok?))
