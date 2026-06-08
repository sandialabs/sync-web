;; Imported from upstream s7test.scm line 35616.
;; Original form:
;; (test (and (values #t) #f) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (and (values #t) #f))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35616 actual expected ok?))
