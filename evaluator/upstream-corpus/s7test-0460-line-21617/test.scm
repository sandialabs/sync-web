;; Imported from upstream s7test.scm line 21617.
;; Original form:
;; (test (format #f "~0D" 123) "123")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "~0D" 123))))
       (expected (upstream-safe (lambda () "123")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21617 actual expected ok?))
