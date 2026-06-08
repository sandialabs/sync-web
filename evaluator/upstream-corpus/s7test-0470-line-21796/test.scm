;; Imported from upstream s7test.scm line 21796.
;; Original form:
;; (test (eq? #\newline ((format #f "\n") 0)) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? #\newline ((format #f "\n") 0)))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21796 actual expected ok?))
