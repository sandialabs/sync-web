;; Imported from upstream s7test.scm line 21606.
;; Original form:
;; (test (format #f "a~~") "a~")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "a~~"))))
       (expected (upstream-safe (lambda () "a~")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21606 actual expected ok?))
