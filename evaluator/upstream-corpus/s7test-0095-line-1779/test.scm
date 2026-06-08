;; Imported from upstream s7test.scm line 1779.
;; Original form:
;; (test (eq? (vector) (vector)) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? (vector) (vector)))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1779 actual expected ok?))
