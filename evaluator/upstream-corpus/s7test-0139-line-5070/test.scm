;; Imported from upstream s7test.scm line 5070.
;; Original form:
;; (test (symbol? (keyword->symbol :if)) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (symbol? (keyword->symbol :if)))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5070 actual expected ok?))
