;; Imported from upstream s7test.scm line 21861.
;; Original form:
;; (test (format #f "~{.~{~A~}+~{~A~}~}" '((1 2) (3 4 5) (6 7 8) (9))) ".12+345.678+9")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "~{.~{~A~}+~{~A~}~}" '((1 2) (3 4 5) (6 7 8) (9))))))
       (expected (upstream-safe (lambda () ".12+345.678+9")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21861 actual expected ok?))
