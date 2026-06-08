;; Imported from upstream s7test.scm line 1761.
;; Original form:
;; (test (eq? #f '  #f) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? #f '  #f))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1761 actual expected ok?))
