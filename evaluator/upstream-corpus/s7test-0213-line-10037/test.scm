;; Imported from upstream s7test.scm line 10037.
;; Original form:
;; (test (((list +) 0) 1 2 3) 6)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (((list +) 0) 1 2 3))))
       (expected (upstream-safe (lambda () 6)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10037 actual expected ok?))
