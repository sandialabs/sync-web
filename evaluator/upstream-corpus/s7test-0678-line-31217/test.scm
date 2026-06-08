;; Imported from upstream s7test.scm line 31217.
;; Original form:
;; (test (and #f 3 asdf) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (and #f 3 asdf))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31217 actual expected ok?))
