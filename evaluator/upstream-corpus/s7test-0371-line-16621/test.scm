;; Imported from upstream s7test.scm line 16621.
;; Original form:
;; (test (catch #t (lambda () (let ((L (list 1))) (L 0 2))) (lambda (typ info) (apply format #f info)))
;;       "((1) 0 2) becomes (1 2), but 1 can't take arguments")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t (lambda () (let ((L (list 1))) (L 0 2))) (lambda (typ info) (apply format #f info))))))
       (expected (upstream-safe (lambda () "((1) 0 2) becomes (1 2), but 1 can't take arguments")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16621 actual expected ok?))
