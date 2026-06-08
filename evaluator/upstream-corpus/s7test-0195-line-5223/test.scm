;; Imported from upstream s7test.scm line 5223.
;; Original form:
;; (test (char? #\1) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (char? #\1))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5223 actual expected ok?))
