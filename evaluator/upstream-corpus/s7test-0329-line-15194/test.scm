;; Imported from upstream s7test.scm line 15194.
;; Original form:
;; (test (equal? '(a b (c . d)) '(a b (c . d))) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (equal? '(a b (c . d)) '(a b (c . d))))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15194 actual expected ok?))
