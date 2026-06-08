;; Imported from upstream s7test.scm line 1769.
;; Original form:
;; (test (eq? *stdin* *stderr*) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? *stdin* *stderr*))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1769 actual expected ok?))
