;; Imported from upstream s7test.scm line 1685.
;; Original form:
;; (test (eq? (null? ()) #t) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? (null? ()) #t))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1685 actual expected ok?))
