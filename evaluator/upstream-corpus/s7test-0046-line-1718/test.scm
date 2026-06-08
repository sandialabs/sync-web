;; Imported from upstream s7test.scm line 1718.
;; Original form:
;; (test (eq? 'abc 'abc) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? 'abc 'abc))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1718 actual expected ok?))
