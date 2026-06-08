;; Imported from upstream s7test.scm line 21615.
;; Original form:
;; (test (format #f "~*~*" 1 2) "")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "~*~*" 1 2))))
       (expected (upstream-safe (lambda () "")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21615 actual expected ok?))
