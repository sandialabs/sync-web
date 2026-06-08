;; Imported from upstream s7test.scm line 1716.
;; Original form:
;; (test (let ((x (lambda () 1))) (let ((y x)) (eq? x y))) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((x (lambda () 1))) (let ((y x)) (eq? x y))))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1716 actual expected ok?))
