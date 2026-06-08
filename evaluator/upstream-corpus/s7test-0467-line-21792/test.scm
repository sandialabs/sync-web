;; Imported from upstream s7test.scm line 21792.
;; Original form:
;; (test (format #f "~%ha") (string-append (string #\newline) "ha"))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "~%ha"))))
       (expected (upstream-safe (lambda () (string-append (string #\newline) "ha"))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21792 actual expected ok?))
