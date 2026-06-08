;; Imported from upstream s7test.scm line 10191.
;; Original form:
;; (test (let* ((lst (list 1 (list 2 3))) (slst lst)) slst) '(1 (2 3)))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let* ((lst (list 1 (list 2 3))) (slst lst)) slst))))
       (expected (upstream-safe (lambda () '(1 (2 3)))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10191 actual expected ok?))
