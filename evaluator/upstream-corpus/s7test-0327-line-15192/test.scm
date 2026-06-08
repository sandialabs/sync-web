;; Imported from upstream s7test.scm line 15192.
;; Original form:
;; (test (equal? '(a) (list 'a)) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (equal? '(a) (list 'a)))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15192 actual expected ok?))
