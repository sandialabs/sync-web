;; Imported from upstream s7test.scm line 25024.
;; Original form:
;; (test (format #f "~W" (lambda (a . b) a)) "(lambda (a . b) a)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "~W" (lambda (a . b) a)))))
       (expected (upstream-safe (lambda () "(lambda (a . b) a)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25024 actual expected ok?))
