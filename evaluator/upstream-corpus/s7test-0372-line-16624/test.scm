;; Imported from upstream s7test.scm line 16624.
;; Original form:
;; (test (catch #t (lambda () (let ((L (list (list 0)))) (L 0 0 2))) (lambda (typ info) (apply format #f info)))
;;       "((0) 0 2) becomes (0 2), but 0 can't take arguments")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t (lambda () (let ((L (list (list 0)))) (L 0 0 2))) (lambda (typ info) (apply format #f info))))))
       (expected (upstream-safe (lambda () "((0) 0 2) becomes (0 2), but 0 can't take arguments")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16624 actual expected ok?))
