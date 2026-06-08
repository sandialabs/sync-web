;; Imported from upstream s7test.scm line 21790.
;; Original form:
;; (test (format #f "hiho~%ha") (string-append "hiho" (string #\newline) "ha"))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "hiho~%ha"))))
       (expected (upstream-safe (lambda () (string-append "hiho" (string #\newline) "ha"))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21790 actual expected ok?))
