;; Imported from upstream s7test.scm line 1765.
;; Original form:
;; (test (let ((f (lambda () (quote (1 . "H"))))) (eq? (f) (f))) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((f (lambda () (quote (1 . "H"))))) (eq? (f) (f))))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1765 actual expected ok?))
