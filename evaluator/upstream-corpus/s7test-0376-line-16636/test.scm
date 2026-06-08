;; Imported from upstream s7test.scm line 16636.
;; Original form:
;; (test (catch #t (lambda () (let ((V (int-vector 1 2))) (V 0 1))) (lambda (typ info) (apply format #f info)))
;;       "vector-ref: too many indices: (0 1)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t (lambda () (let ((V (int-vector 1 2))) (V 0 1))) (lambda (typ info) (apply format #f info))))))
       (expected (upstream-safe (lambda () "vector-ref: too many indices: (0 1)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16636 actual expected ok?))
