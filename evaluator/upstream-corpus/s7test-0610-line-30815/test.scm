;; Imported from upstream s7test.scm line 30815.
;; Original form:
;; (test (catch #t (lambda () (set! (_not_a_pws_) 1)) (lambda (typ info) (apply format #f info))) "unbound variable _not_a_pws_ in (set! (_not_a_pws_) 1)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t (lambda () (set! (_not_a_pws_) 1)) (lambda (typ info) (apply format #f info))))))
       (expected (upstream-safe (lambda () "unbound variable _not_a_pws_ in (set! (_not_a_pws_) 1)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 30815 actual expected ok?))
