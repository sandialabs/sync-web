;; Imported from upstream s7test.scm line 30837.
;; Original form:
;; (test (let ((var 1) (val 2)) (set! var +) (var val 3)) 5)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((var 1) (val 2)) (set! var +) (var val 3)))))
       (expected (upstream-safe (lambda () 5)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 30837 actual expected ok?))
