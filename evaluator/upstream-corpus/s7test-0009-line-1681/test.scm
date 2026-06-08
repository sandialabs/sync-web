;; Imported from upstream s7test.scm line 1681.
;; Original form:
;; (test (eq? #\a #\b) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? #\a #\b))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1681 actual expected ok?))
