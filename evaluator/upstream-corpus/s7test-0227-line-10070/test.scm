;; Imported from upstream s7test.scm line 10070.
;; Original form:
;; (test (fill! () 1) 1)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (fill! () 1))))
       (expected (upstream-safe (lambda () 1)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10070 actual expected ok?))
