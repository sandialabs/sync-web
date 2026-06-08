;; Imported from upstream s7test.scm line 20478.
;; Original form:
;; (test (length *stderr*) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (length *stderr*))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 20478 actual expected ok?))
