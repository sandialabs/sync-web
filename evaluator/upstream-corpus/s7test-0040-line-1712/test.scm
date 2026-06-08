;; Imported from upstream s7test.scm line 1712.
;; Original form:
;; (test (eq? (vector 'a) (vector 'a)) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? (vector 'a) (vector 'a)))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1712 actual expected ok?))
