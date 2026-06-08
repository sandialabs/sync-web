;; Imported from upstream s7test.scm line 21616.
;; Original form:
;; (test (format #f "~20,'~D" 3) "~~~~~~~~~~~~~~~~~~~3")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "~20,'~D" 3))))
       (expected (upstream-safe (lambda () "~~~~~~~~~~~~~~~~~~~3")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21616 actual expected ok?))
