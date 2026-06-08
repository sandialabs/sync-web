;; Imported from upstream s7test.scm line 21580.
;; Original form:
;; (test (with-output-to-string (lambda () (newline #f))) "")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (with-output-to-string (lambda () (newline #f))))))
       (expected (upstream-safe (lambda () "")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21580 actual expected ok?))
