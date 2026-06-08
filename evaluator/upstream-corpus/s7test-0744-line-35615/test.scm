;; Imported from upstream s7test.scm line 35615.
;; Original form:
;; (test (or (values #t) #f) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (or (values #t) #f))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35615 actual expected ok?))
