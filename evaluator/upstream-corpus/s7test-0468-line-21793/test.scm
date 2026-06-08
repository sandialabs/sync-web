;; Imported from upstream s7test.scm line 21793.
;; Original form:
;; (test (format #f "hiho~%") (string-append "hiho" (string #\newline)))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "hiho~%"))))
       (expected (upstream-safe (lambda () (string-append "hiho" (string #\newline)))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21793 actual expected ok?))
