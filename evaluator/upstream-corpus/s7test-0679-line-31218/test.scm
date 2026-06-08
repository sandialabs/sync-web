;; Imported from upstream s7test.scm line 31218.
;; Original form:
;; (test (and 3 (zero? 1) (/ 1 0) (display "and is about to exit!") (exit)) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (and 3 (zero? 1) (/ 1 0) (display "and is about to exit!") (exit)))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31218 actual expected ok?))
