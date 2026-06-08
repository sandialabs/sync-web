;; Imported from upstream s7test.scm line 20086.
;; Original form:
;; (test (defined? 'j0) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (defined? 'j0))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 20086 actual expected ok?))
