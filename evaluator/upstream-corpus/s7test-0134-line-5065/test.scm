;; Imported from upstream s7test.scm line 5065.
;; Original form:
;; (test (symbol? 'sym0123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (symbol? 'sym0123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5065 actual expected ok?))
