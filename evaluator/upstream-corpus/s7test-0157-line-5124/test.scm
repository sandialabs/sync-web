;; Imported from upstream s7test.scm line 5124.
;; Original form:
;; (test (syntax? 'lambda) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (syntax? 'lambda))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5124 actual expected ok?))
