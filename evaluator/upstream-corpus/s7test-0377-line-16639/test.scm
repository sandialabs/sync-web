;; Imported from upstream s7test.scm line 16639.
;; Original form:
;; (test (catch #t (lambda () (let ((L (inlet))) (L 'a :asdf))) (lambda (typ info) (apply format #f info)))
;;       "((inlet) 'a :asdf) becomes (#<undefined> :asdf), but #<undefined> can't take arguments")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t (lambda () (let ((L (inlet))) (L 'a :asdf))) (lambda (typ info) (apply format #f info))))))
       (expected (upstream-safe (lambda () "((inlet) 'a :asdf) becomes (#<undefined> :asdf), but #<undefined> can't take arguments")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16639 actual expected ok?))
