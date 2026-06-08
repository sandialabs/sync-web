;; Imported from upstream s7test.scm line 21851.
;; Original form:
;; (test (format #f "~{~A~}" '(1 2 3)) "123")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "~{~A~}" '(1 2 3)))))
       (expected (upstream-safe (lambda () "123")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21851 actual expected ok?))
