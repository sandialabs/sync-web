;; Imported from upstream s7test.scm line 21856.
;; Original form:
;; (test (format #f "~{~A ~A ~}" '(1 "hi" 2 "ho")) "1 hi 2 ho ")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "~{~A ~A ~}" '(1 "hi" 2 "ho")))))
       (expected (upstream-safe (lambda () "1 hi 2 ho ")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21856 actual expected ok?))
