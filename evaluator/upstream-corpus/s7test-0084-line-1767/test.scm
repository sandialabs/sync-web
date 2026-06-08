;; Imported from upstream s7test.scm line 1767.
;; Original form:
;; (test (eq? *stdin* *stdin*) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? *stdin* *stdin*))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1767 actual expected ok?))
