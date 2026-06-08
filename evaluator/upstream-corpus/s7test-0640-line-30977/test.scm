;; Imported from upstream s7test.scm line 30977.
;; Original form:
;; (test (let ((x (list 1 2))) (let ((y x)) (set! (x 0) (define x (vector 2 3))) (list x y))) '(#(2 3) (#(2 3) 2)))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((x (list 1 2))) (let ((y x)) (set! (x 0) (define x (vector 2 3))) (list x y))))))
       (expected (upstream-safe (lambda () '(#(2 3) (#(2 3) 2)))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 30977 actual expected ok?))
