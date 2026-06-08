;; Imported from upstream s7test.scm line 21579.
;; Original form:
;; (test (with-output-to-string (lambda () (newline))) "\n")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (with-output-to-string (lambda () (newline))))))
       (expected (upstream-safe (lambda () "\n")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21579 actual expected ok?))
