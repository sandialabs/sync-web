;; Imported from upstream s7test.scm line 15001.
;; Original form:
;; (test (equal? (vector 0) (vector 0)) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (equal? (vector 0) (vector 0)))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15001 actual expected ok?))
