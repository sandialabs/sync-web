;; Imported from upstream s7test.scm line 5098.
;; Original form:
;; (test (symbol? '1.2.3) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (symbol? '1.2.3))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5098 actual expected ok?))
