;; Imported from upstream s7test.scm line 1768.
;; Original form:
;; (test (eq? *stdout* *stderr*) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? *stdout* *stderr*))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1768 actual expected ok?))
