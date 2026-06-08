;; Imported from upstream s7test.scm line 1780.
;; Original form:
;; (test (eq? (vector) #()) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? (vector) #()))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1780 actual expected ok?))
