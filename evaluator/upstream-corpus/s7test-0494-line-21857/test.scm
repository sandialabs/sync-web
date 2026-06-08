;; Imported from upstream s7test.scm line 21857.
;; Original form:
;; (test (format #f "~{.~{+~A+~}.~}" (list (list 1 2 3) (list 4 5 6))) ".+1++2++3+..+4++5++6+.")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "~{.~{+~A+~}.~}" (list (list 1 2 3) (list 4 5 6))))))
       (expected (upstream-safe (lambda () ".+1++2++3+..+4++5++6+.")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21857 actual expected ok?))
