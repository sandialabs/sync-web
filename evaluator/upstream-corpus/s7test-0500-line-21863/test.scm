;; Imported from upstream s7test.scm line 21863.
;; Original form:
;; (test (format #f "~{.~{+~{-~A~}~}~}" '(((1 2) (3 4 5)) ((6) (7 8 9)))) ".+-1-2+-3-4-5.+-6+-7-8-9")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "~{.~{+~{-~A~}~}~}" '(((1 2) (3 4 5)) ((6) (7 8 9)))))))
       (expected (upstream-safe (lambda () ".+-1-2+-3-4-5.+-6+-7-8-9")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21863 actual expected ok?))
