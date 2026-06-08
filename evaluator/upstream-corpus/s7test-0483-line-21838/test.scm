;; Imported from upstream s7test.scm line 21838.
;; Original form:
;; (test (format #f "~nc" 0 #\a) "")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "~nc" 0 #\a))))
       (expected (upstream-safe (lambda () "")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21838 actual expected ok?))
