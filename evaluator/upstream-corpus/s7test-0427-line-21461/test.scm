;; Imported from upstream s7test.scm line 21461.
;; Original form:
;; (test (with-output-to-string (lambda () (write (string (integer->char 4) (integer->char 8) (integer->char 20) (integer->char 30))))) "\"\\x04;\\b\\x14;\\x1e;\"")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (with-output-to-string (lambda () (write (string (integer->char 4) (integer->char 8) (integer->char 20) (integer->char 30))))))))
       (expected (upstream-safe (lambda () "\"\\x04;\\b\\x14;\\x1e;\"")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21461 actual expected ok?))
