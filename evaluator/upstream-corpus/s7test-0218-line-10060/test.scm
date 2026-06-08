;; Imported from upstream s7test.scm line 10060.
;; Original form:
;; (test (let ((x ''foo)) (list-set! x 0 "hi") x ) '("hi" foo))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((x ''foo)) (list-set! x 0 "hi") x ))))
       (expected (upstream-safe (lambda () '("hi" foo))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10060 actual expected ok?))
