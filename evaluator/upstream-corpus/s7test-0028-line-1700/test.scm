;; Imported from upstream s7test.scm line 1700.
;; Original form:
;; (test (eq? 'a (symbol "a")) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? 'a (symbol "a")))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1700 actual expected ok?))
