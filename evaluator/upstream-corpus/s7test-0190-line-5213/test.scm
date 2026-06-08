;; Imported from upstream s7test.scm line 5213.
;; Original form:
;; (test (eqv? '#\  #\space) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eqv? '#\  #\space))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5213 actual expected ok?))
