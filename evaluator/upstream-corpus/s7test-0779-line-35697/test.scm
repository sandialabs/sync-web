;; Imported from upstream s7test.scm line 35697.
;; Original form:
;; (test ((lambda () (let ((x 1)) (set! x (boolean? (values)))))) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () ((lambda () (let ((x 1)) (set! x (boolean? (values)))))))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35697 actual expected ok?))
