;; Imported from upstream s7test.scm line 35725.
;; Original form:
;; (test (catch #t (lambda () (let ((x 1)) (let-temporarily ((x (values 1 2))) x))) (lambda (type info) (apply format #f info))) "set!: can't set x to (values 1 2)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t (lambda () (let ((x 1)) (let-temporarily ((x (values 1 2))) x))) (lambda (type info) (apply format #f info))))))
       (expected (upstream-safe (lambda () "set!: can't set x to (values 1 2)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35725 actual expected ok?))
