;; Imported from upstream s7test.scm line 1679.
;; Original form:
;; (test (eq? '(1) '(1)) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? '(1) '(1)))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1679 actual expected ok?))
