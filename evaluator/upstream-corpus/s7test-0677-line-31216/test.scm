;; Imported from upstream s7test.scm line 31216.
;; Original form:
;; (test (and 3 9) 9)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (and 3 9))))
       (expected (upstream-safe (lambda () 9)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31216 actual expected ok?))
