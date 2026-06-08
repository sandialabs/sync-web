;; Imported from upstream s7test.scm line 31156.
;; Original form:
;; (test (or 3 (/ 1 0) (display "or is about to exit!") (exit)) 3)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (or 3 (/ 1 0) (display "or is about to exit!") (exit)))))
       (expected (upstream-safe (lambda () 3)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31156 actual expected ok?))
