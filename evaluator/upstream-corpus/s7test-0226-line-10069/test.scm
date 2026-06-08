;; Imported from upstream s7test.scm line 10069.
;; Original form:
;; (test (fill! (list 1 2) 4) 4)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (fill! (list 1 2) 4))))
       (expected (upstream-safe (lambda () 4)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10069 actual expected ok?))
