;; Imported from upstream s7test.scm line 21017.
;; Original form:
;; (test (with-input-from-string "(+ 1 2 3)" (lambda () (*s7* 'version))) (*s7* 'version))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (with-input-from-string "(+ 1 2 3)" (lambda () (*s7* 'version))))))
       (expected (upstream-safe (lambda () (*s7* 'version))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21017 actual expected ok?))
