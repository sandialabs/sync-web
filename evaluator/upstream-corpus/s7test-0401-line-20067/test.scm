;; Imported from upstream s7test.scm line 20067.
;; Original form:
;; (test (defined? 'auto_test_var) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (defined? 'auto_test_var))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 20067 actual expected ok?))
