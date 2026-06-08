;; Imported from upstream s7test.scm line 5094.
;; Original form:
;; (test (symbol? and) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (symbol? and))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5094 actual expected ok?))
