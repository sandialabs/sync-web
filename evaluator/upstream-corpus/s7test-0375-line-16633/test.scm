;; Imported from upstream s7test.scm line 16633.
;; Original form:
;; (test (catch #t (lambda () (let ((V (vector (vector 0 12)))) (V 0 1 0))) (lambda (typ info) (apply format #f info)))
;;       "(#(0 12) 1 0) becomes (12 0), but 12 can't take arguments")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t (lambda () (let ((V (vector (vector 0 12)))) (V 0 1 0))) (lambda (typ info) (apply format #f info))))))
       (expected (upstream-safe (lambda () "(#(0 12) 1 0) becomes (12 0), but 12 can't take arguments")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16633 actual expected ok?))
