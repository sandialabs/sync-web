;; Imported from upstream s7test.scm line 21852.
;; Original form:
;; (test (format #f "asb~{~A ~}asb" '(1 2 3 4)) "asb1 2 3 4 asb")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "asb~{~A ~}asb" '(1 2 3 4)))))
       (expected (upstream-safe (lambda () "asb1 2 3 4 asb")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21852 actual expected ok?))
