;; Imported from upstream s7test.scm line 31180.
;; Original form:
;; (test (or (or (or) (and))) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (or (or (or) (and))))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31180 actual expected ok?))
