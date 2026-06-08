;; Imported from upstream s7test.scm line 16645.
;; Original form:
;; (test (catch #t (lambda () (let ((L (list 1))) (set! (L 0 2) 32))) (lambda (typ info) (apply format #f info)))
;;       "in (set! (L 0 2) 32), ((1) 0) is 1 which can't take arguments")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t (lambda () (let ((L (list 1))) (set! (L 0 2) 32))) (lambda (typ info) (apply format #f info))))))
       (expected (upstream-safe (lambda () "in (set! (L 0 2) 32), ((1) 0) is 1 which can't take arguments")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16645 actual expected ok?))
