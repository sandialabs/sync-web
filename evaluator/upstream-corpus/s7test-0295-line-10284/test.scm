;; Imported from upstream s7test.scm line 10284.
;; Original form:
;; (test (make-list 1 123) '(123))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (make-list 1 123))))
       (expected (upstream-safe (lambda () '(123))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10284 actual expected ok?))
