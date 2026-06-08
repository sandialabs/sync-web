;; Imported from upstream s7test.scm line 21605.
;; Original form:
;; (test (format #f "~~~~") "~~")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "~~~~"))))
       (expected (upstream-safe (lambda () "~~")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21605 actual expected ok?))
