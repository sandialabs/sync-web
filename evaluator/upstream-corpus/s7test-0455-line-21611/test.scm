;; Imported from upstream s7test.scm line 21611.
;; Original form:
;; (test (format #f "~P" 1) "")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "~P" 1))))
       (expected (upstream-safe (lambda () "")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21611 actual expected ok?))
