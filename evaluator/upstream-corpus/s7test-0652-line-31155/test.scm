;; Imported from upstream s7test.scm line 31155.
;; Original form:
;; (test (or #f 3 asdf) 3)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (or #f 3 asdf))))
       (expected (upstream-safe (lambda () 3)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31155 actual expected ok?))
