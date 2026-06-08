;; Imported from upstream s7test.scm line 10282.
;; Original form:
;; (test (make-list 0 123) ())

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (make-list 0 123))))
       (expected (upstream-safe (lambda () ())))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10282 actual expected ok?))
