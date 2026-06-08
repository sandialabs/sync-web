;; Imported from upstream s7test.scm line 35581.
;; Original form:
;; (test (if (values () 1) 3 4) 3)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (if (values () 1) 3 4))))
       (expected (upstream-safe (lambda () 3)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35581 actual expected ok?))
