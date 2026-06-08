;; Imported from upstream s7test.scm line 21618.
;; Original form:
;; (test (format #f "~{~S~}" ()) "")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "~{~S~}" ()))))
       (expected (upstream-safe (lambda () "")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21618 actual expected ok?))
