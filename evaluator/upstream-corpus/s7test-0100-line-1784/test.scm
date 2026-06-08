;; Imported from upstream s7test.scm line 1784.
;; Original form:
;; (test (eq? (curlet) (curlet)) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? (curlet) (curlet)))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1784 actual expected ok?))
