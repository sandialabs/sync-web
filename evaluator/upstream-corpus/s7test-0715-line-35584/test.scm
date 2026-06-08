;; Imported from upstream s7test.scm line 35584.
;; Original form:
;; (test (if (values #f #t) 1) #<unspecified>)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (if (values #f #t) 1))))
       (expected (upstream-safe (lambda () #<unspecified>)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35584 actual expected ok?))
