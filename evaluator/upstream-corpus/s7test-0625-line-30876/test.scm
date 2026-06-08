;; Imported from upstream s7test.scm line 30876.
;; Original form:
;; (test (set! ('(a 0) 1) 0) 0)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (set! ('(a 0) 1) 0))))
       (expected (upstream-safe (lambda () 0)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 30876 actual expected ok?))
