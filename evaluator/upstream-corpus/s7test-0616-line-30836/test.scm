;; Imported from upstream s7test.scm line 30836.
;; Original form:
;; (test (let ((var 1) (val 2)) (set! var set!) (var val 3) val) 3)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((var 1) (val 2)) (set! var set!) (var val 3) val))))
       (expected (upstream-safe (lambda () 3)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 30836 actual expected ok?))
