;; Imported from upstream s7test.scm line 1719.
;; Original form:
;; (test (eq? eq? eq?) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? eq? eq?))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1719 actual expected ok?))
