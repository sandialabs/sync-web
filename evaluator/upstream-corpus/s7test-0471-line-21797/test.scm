;; Imported from upstream s7test.scm line 21797.
;; Original form:
;; (test (eq? #\\ ((format #f "\\") 0)) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? #\\ ((format #f "\\") 0)))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21797 actual expected ok?))
