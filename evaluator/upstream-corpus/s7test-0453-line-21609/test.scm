;; Imported from upstream s7test.scm line 21609.
;; Original form:
;; (test (format #f "~{~^~A~}" ()) "")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "~{~^~A~}" ()))))
       (expected (upstream-safe (lambda () "")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21609 actual expected ok?))
