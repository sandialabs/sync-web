;; Imported from upstream s7test.scm line 1720.
;; Original form:
;; (test (eq? (if #f 1) 1) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? (if #f 1) 1))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1720 actual expected ok?))
