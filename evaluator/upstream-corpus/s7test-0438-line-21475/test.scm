;; Imported from upstream s7test.scm line 21475.
;; Original form:
;; (test (with-output-to-string (lambda () (write '{))) "{")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (with-output-to-string (lambda () (write '{))))))
       (expected (upstream-safe (lambda () "{")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21475 actual expected ok?))
