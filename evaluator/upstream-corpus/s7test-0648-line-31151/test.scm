;; Imported from upstream s7test.scm line 31151.
;; Original form:
;; (test (or #f #f #f) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (or #f #f #f))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31151 actual expected ok?))
