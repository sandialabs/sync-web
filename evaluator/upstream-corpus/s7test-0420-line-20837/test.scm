;; Imported from upstream s7test.scm line 20837.
;; Original form:
;; (test (with-input-from-string "" read) #<eof>)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (with-input-from-string "" read))))
       (expected (upstream-safe (lambda () #<eof>)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 20837 actual expected ok?))
