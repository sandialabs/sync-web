;; Imported from upstream s7test.scm line 35586.
;; Original form:
;; (test (equal? (values #t #t)) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (equal? (values #t #t)))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35586 actual expected ok?))
