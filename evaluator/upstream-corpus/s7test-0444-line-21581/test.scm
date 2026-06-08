;; Imported from upstream s7test.scm line 21581.
;; Original form:
;; (test (call-with-output-string (lambda (p) (newline p))) "\n")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (call-with-output-string (lambda (p) (newline p))))))
       (expected (upstream-safe (lambda () "\n")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21581 actual expected ok?))
