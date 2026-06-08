;; Imported from upstream s7test.scm line 5140.
;; Original form:
;; (test (syntax? 'else) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (syntax? 'else))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5140 actual expected ok?))
