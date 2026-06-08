;; Imported from upstream s7test.scm line 1735.
;; Original form:
;; (test (eq? 3/4 3) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? 3/4 3))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1735 actual expected ok?))
