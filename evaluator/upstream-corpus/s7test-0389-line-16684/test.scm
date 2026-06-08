;; Imported from upstream s7test.scm line 16684.
;; Original form:
;; (test (catch #t (lambda () (let ((L (inlet))) (let-ref L 'a :asdf))) (lambda (type info) (apply format #f info)))
;;       "let-ref: too many arguments: (let-ref (inlet) a :asdf)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t (lambda () (let ((L (inlet))) (let-ref L 'a :asdf))) (lambda (type info) (apply format #f info))))))
       (expected (upstream-safe (lambda () "let-ref: too many arguments: (let-ref (inlet) a :asdf)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16684 actual expected ok?))
