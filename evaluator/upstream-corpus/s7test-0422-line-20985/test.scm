;; Imported from upstream s7test.scm line 20985.
;; Original form:
;; (test (with-output-to-string (lambda () (*s7* 'version))) "")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (with-output-to-string (lambda () (*s7* 'version))))))
       (expected (upstream-safe (lambda () "")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 20985 actual expected ok?))
