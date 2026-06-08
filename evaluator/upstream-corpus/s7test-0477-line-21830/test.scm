;; Imported from upstream s7test.scm line 21830.
;; Original form:
;; (test (format #f "hi ~S ho" 1) "hi 1 ho")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "hi ~S ho" 1))))
       (expected (upstream-safe (lambda () "hi 1 ho")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21830 actual expected ok?))
