;; Imported from upstream s7test.scm line 10075.
;; Original form:
;; (test (let ((lst '(1 2 3))) (set! (lst 0) 32)) 32)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((lst '(1 2 3))) (set! (lst 0) 32)))))
       (expected (upstream-safe (lambda () 32)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10075 actual expected ok?))
