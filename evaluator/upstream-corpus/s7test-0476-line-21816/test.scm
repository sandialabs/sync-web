;; Imported from upstream s7test.scm line 21816.
;; Original form:
;; (test (format #f "~a~%~a" 1 3) (string-append "1" (string #\newline) "3"))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "~a~%~a" 1 3))))
       (expected (upstream-safe (lambda () (string-append "1" (string #\newline) "3"))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21816 actual expected ok?))
