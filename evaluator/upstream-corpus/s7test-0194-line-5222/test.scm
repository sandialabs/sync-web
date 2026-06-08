;; Imported from upstream s7test.scm line 5222.
;; Original form:
;; (test (char? '#\newline) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (char? '#\newline))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5222 actual expected ok?))
