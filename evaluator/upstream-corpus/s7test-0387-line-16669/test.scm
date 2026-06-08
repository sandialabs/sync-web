;; Imported from upstream s7test.scm line 16669.
;; Original form:
;; (test (catch #t (lambda () (let ((L (inlet))) (set! (L 'a :asdf) 32))) (lambda (typ info) (apply format #f info)))
;;       "in (set! (L 'a :asdf) 32), ((inlet) 'a) is #<undefined> which can't take arguments")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t (lambda () (let ((L (inlet))) (set! (L 'a :asdf) 32))) (lambda (typ info) (apply format #f info))))))
       (expected (upstream-safe (lambda () "in (set! (L 'a :asdf) 32), ((inlet) 'a) is #<undefined> which can't take arguments")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16669 actual expected ok?))
