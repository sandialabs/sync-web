;; Imported from upstream s7test.scm line 5125.
;; Original form:
;; (test (syntax? lambda) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (syntax? lambda))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5125 actual expected ok?))
