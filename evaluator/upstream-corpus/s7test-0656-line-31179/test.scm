;; Imported from upstream s7test.scm line 31179.
;; Original form:
;; (test (or (or (or))) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (or (or (or))))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31179 actual expected ok?))
