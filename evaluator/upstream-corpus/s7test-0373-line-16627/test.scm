;; Imported from upstream s7test.scm line 16627.
;; Original form:
;; (test (catch #t (lambda () (let ((V (vector 1 2))) (V 0 1))) (lambda (typ info) (apply format #f info)))
;;       "(#(1 2) 0 1) becomes (1 1), but 1 can't take arguments")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t (lambda () (let ((V (vector 1 2))) (V 0 1))) (lambda (typ info) (apply format #f info))))))
       (expected (upstream-safe (lambda () "(#(1 2) 0 1) becomes (1 1), but 1 can't take arguments")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16627 actual expected ok?))
