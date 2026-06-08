;; Imported from upstream s7test.scm line 16642.
;; Original form:
;; (test (catch #t (lambda () (let ((L (inlet 'a (inlet 'b 1)))) (L 'a 'b 'c))) (lambda (typ info) (apply format #f info)))
;;       "((inlet 'b 1) 'b 'c) becomes (1 'c), but 1 can't take arguments")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t (lambda () (let ((L (inlet 'a (inlet 'b 1)))) (L 'a 'b 'c))) (lambda (typ info) (apply format #f info))))))
       (expected (upstream-safe (lambda () "((inlet 'b 1) 'b 'c) becomes (1 'c), but 1 can't take arguments")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16642 actual expected ok?))
