;; Imported from upstream s7test.scm line 5067.
;; Original form:
;; (test (if (symbol? '1+) (symbol? '0e) #t) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (if (symbol? '1+) (symbol? '0e) #t))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5067 actual expected ok?))
