;; Imported from upstream s7test.scm line 1750.
;; Original form:
;; (test (eq? '2 2) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? '2 2))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1750 actual expected ok?))
