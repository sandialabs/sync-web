;; Imported from upstream s7test.scm line 16666.
;; Original form:
;; (test (catch #t (lambda () (let ((L (inlet 'a (inlet 'b 1)))) (set! (L 'a 'b 'c) 32))) (lambda (typ info) (apply format #f info)))
;;       "in (set! (L 'a 'b 'c) 32), ((inlet 'b 1) 'b) is 1 which can't take arguments")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t (lambda () (let ((L (inlet 'a (inlet 'b 1)))) (set! (L 'a 'b 'c) 32))) (lambda (typ info) (apply format #f info))))))
       (expected (upstream-safe (lambda () "in (set! (L 'a 'b 'c) 32), ((inlet 'b 1) 'b) is 1 which can't take arguments")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16666 actual expected ok?))
