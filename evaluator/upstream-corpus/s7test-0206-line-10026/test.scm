;; Imported from upstream s7test.scm line 10026.
;; Original form:
;; (test ((append '(3) () 1) 0) 3)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () ((append '(3) () 1) 0))))
       (expected (upstream-safe (lambda () 3)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10026 actual expected ok?))
