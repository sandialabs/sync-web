;; Imported from upstream s7test.scm line 1723.
;; Original form:
;; (test (eq? '#||#hi 'hi) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? '#||#hi 'hi))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1723 actual expected ok?))
