;; Imported from upstream s7test.scm line 1753.
;; Original form:
;; (test (eq? '#\a #\a) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? '#\a #\a))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1753 actual expected ok?))
