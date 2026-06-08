;; Imported from upstream s7test.scm line 1717.
;; Original form:
;; (test (let ((x (lambda () 1))) (let ((y (lambda () 1))) (eq? x y))) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((x (lambda () 1))) (let ((y (lambda () 1))) (eq? x y))))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1717 actual expected ok?))
