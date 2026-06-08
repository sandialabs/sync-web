;; Imported from upstream s7test.scm line 25023.
;; Original form:
;; (test (format #f "~W" (lambda args args)) "(lambda args args)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "~W" (lambda args args)))))
       (expected (upstream-safe (lambda () "(lambda args args)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25023 actual expected ok?))
