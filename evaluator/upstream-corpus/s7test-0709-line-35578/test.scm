;; Imported from upstream s7test.scm line 35578.
;; Original form:
;; (test (if (values #f) 1 2) 2)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (if (values #f) 1 2))))
       (expected (upstream-safe (lambda () 2)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35578 actual expected ok?))
