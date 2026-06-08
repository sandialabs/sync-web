;; Imported from upstream s7test.scm line 16693.
;; Original form:
;; (test (catch #t (lambda () (let ((V (vector 1 2))) (set! (V 0 1) 32))) (lambda (type info) (apply format #f info)))
;;       "in (set! (V 0 1) 32), (#(1 2) 0) is 1 which can't take arguments")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t (lambda () (let ((V (vector 1 2))) (set! (V 0 1) 32))) (lambda (type info) (apply format #f info))))))
       (expected (upstream-safe (lambda () "in (set! (V 0 1) 32), (#(1 2) 0) is 1 which can't take arguments")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16693 actual expected ok?))
