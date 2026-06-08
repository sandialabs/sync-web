;; Imported from upstream s7test.scm line 21831.
;; Original form:
;; (test (format #f "hi ~S ho" "abc") "hi \"abc\" ho")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "hi ~S ho" "abc"))))
       (expected (upstream-safe (lambda () "hi \"abc\" ho")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21831 actual expected ok?))
