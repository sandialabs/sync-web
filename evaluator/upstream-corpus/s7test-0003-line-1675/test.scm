;; Imported from upstream s7test.scm line 1675.
;; Original form:
;; (test (eq? "abs" 'abc) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? "abs" 'abc))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1675 actual expected ok?))
