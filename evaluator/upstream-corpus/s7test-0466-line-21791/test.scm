;; Imported from upstream s7test.scm line 21791.
;; Original form:
;; (test (format #f "~%") (string #\newline))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "~%"))))
       (expected (upstream-safe (lambda () (string #\newline))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21791 actual expected ok?))
