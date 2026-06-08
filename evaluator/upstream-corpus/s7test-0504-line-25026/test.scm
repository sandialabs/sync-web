;; Imported from upstream s7test.scm line 25026.
;; Original form:
;; (test (format #f "~W" (lambda (a b . c) a)) "(lambda (a b . c) a)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "~W" (lambda (a b . c) a)))))
       (expected (upstream-safe (lambda () "(lambda (a b . c) a)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25026 actual expected ok?))
