;; Imported from upstream s7test.scm line 31153.
;; Original form:
;; (test (or (memq 'b '(a b c)) (+ 3 0)) '(b c))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (or (memq 'b '(a b c)) (+ 3 0)))))
       (expected (upstream-safe (lambda () '(b c))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31153 actual expected ok?))
