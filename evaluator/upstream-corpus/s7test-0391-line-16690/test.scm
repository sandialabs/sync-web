;; Imported from upstream s7test.scm line 16690.
;; Original form:
;; (test (catch #t (lambda () (let ((V (vector 1 2))) (vector-set! V 0 1 32))) (lambda (type info) (apply format #f info)))
;;       "too many arguments for vector-set!: (#(1 2) 0 1 32)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t (lambda () (let ((V (vector 1 2))) (vector-set! V 0 1 32))) (lambda (type info) (apply format #f info))))))
       (expected (upstream-safe (lambda () "too many arguments for vector-set!: (#(1 2) 0 1 32)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16690 actual expected ok?))
