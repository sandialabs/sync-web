;; Imported from upstream s7test.scm line 21467.
;; Original form:
;; (test (string=? "\x61;\x42;\x63;" "aBc") #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (string=? "\x61;\x42;\x63;" "aBc"))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21467 actual expected ok?))
