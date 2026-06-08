;; Imported from upstream s7test.scm line 21812.
;; Original form:
;; (test (format #f "hi ~A ho" 1) "hi 1 ho")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "hi ~A ho" 1))))
       (expected (upstream-safe (lambda () "hi 1 ho")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21812 actual expected ok?))
