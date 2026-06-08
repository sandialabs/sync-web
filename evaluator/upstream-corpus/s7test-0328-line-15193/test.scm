;; Imported from upstream s7test.scm line 15193.
;; Original form:
;; (test (equal? '(a b . c) '(a b . c)) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (equal? '(a b . c) '(a b . c)))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15193 actual expected ok?))
