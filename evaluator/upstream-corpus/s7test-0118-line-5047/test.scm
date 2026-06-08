;; Imported from upstream s7test.scm line 5047.
;; Original form:
;; (test (symbol? 'nil) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (symbol? 'nil))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5047 actual expected ok?))
