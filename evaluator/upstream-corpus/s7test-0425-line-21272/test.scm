;; Imported from upstream s7test.scm line 21272.
;; Original form:
;; (test (with-input-from-string "123" (lambda () (read-byte))) 49)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (with-input-from-string "123" (lambda () (read-byte))))))
       (expected (upstream-safe (lambda () 49)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21272 actual expected ok?))
