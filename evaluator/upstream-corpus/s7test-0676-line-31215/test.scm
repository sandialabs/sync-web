;; Imported from upstream s7test.scm line 31215.
;; Original form:
;; (test (and (memq 'b '(a b c)) (+ 3 0)) 3)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (and (memq 'b '(a b c)) (+ 3 0)))))
       (expected (upstream-safe (lambda () 3)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31215 actual expected ok?))
