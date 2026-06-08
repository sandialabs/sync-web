;; Imported from upstream s7test.scm line 25025.
;; Original form:
;; (test (format #f "~W" (lambda* (a b :rest c) a)) "(lambda* (a b :rest c) a)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "~W" (lambda* (a b :rest c) a)))))
       (expected (upstream-safe (lambda () "(lambda* (a b :rest c) a)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25025 actual expected ok?))
