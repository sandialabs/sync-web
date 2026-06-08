;; Imported from upstream s7test.scm line 1686.
;; Original form:
;; (test (eq? (null? '(a)) #f) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? (null? '(a)) #f))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1686 actual expected ok?))
