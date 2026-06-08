;; Imported from upstream s7test.scm line 5214.
;; Original form:
;; (test (eqv? #\newline '#\newline) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eqv? #\newline '#\newline))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5214 actual expected ok?))
