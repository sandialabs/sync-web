;; Imported from upstream s7test.scm line 21680.
;; Original form:
;; (test (format #f "~P" (real-part (log 0))) "s")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "~P" (real-part (log 0))))))
       (expected (upstream-safe (lambda () "s")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21680 actual expected ok?))
