;; Imported from upstream s7test.scm line 21862.
;; Original form:
;; (test (format #f "~{.~{+~{-~A~}~}~}" '(((1 2) (3 4 5)))) ".+-1-2+-3-4-5")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "~{.~{+~{-~A~}~}~}" '(((1 2) (3 4 5)))))))
       (expected (upstream-safe (lambda () ".+-1-2+-3-4-5")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21862 actual expected ok?))
