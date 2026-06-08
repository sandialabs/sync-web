;; Imported from upstream s7test.scm line 30860.
;; Original form:
;; (test (set! ('(1 2) 0) 3) 3)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (set! ('(1 2) 0) 3))))
       (expected (upstream-safe (lambda () 3)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 30860 actual expected ok?))
