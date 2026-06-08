;; Imported from upstream s7test.scm line 5229.
;; Original form:
;; (test (char? '#\space) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (char? '#\space))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5229 actual expected ok?))
