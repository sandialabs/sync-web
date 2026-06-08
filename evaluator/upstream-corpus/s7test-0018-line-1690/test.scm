;; Imported from upstream s7test.scm line 1690.
;; Original form:
;; (test (eq? 'a (string->symbol "a")) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? 'a (string->symbol "a")))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1690 actual expected ok?))
