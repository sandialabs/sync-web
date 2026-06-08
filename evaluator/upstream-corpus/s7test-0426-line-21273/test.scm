;; Imported from upstream s7test.scm line 21273.
;; Original form:
;; (test (nan? (with-input-from-string "1/0" read)) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (nan? (with-input-from-string "1/0" read)))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21273 actual expected ok?))
