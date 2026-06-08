;; Imported from upstream s7test.scm line 31247.
;; Original form:
;; (test (and (and (and (and (or))))) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (and (and (and (and (or))))))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31247 actual expected ok?))
