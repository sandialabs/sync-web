;; Imported from upstream s7test.scm line 1741.
;; Original form:
;; (test (eq? (begin) (append)) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? (begin) (append)))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1741 actual expected ok?))
