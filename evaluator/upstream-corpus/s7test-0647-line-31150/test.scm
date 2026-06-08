;; Imported from upstream s7test.scm line 31150.
;; Original form:
;; (test (or (= 2 2) (< 2 1)) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (or (= 2 2) (< 2 1)))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31150 actual expected ok?))
