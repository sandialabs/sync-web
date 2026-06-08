;; Imported from upstream s7test.scm line 31245.
;; Original form:
;; (test (and and) and)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (and and))))
       (expected (upstream-safe (lambda () and)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31245 actual expected ok?))
