;; Imported from upstream s7test.scm line 1707.
;; Original form:
;; (test (eq? "abc" "cba") #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? "abc" "cba"))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1707 actual expected ok?))
