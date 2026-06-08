;; Imported from upstream s7test.scm line 35722.
;; Original form:
;; (test (catch #t (lambda () (letrec ((y 1) (x (values 1 2))) x)) (lambda (type info) (apply format #f info))) "letrec: can't bind x to (values 1 2)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t (lambda () (letrec ((y 1) (x (values 1 2))) x)) (lambda (type info) (apply format #f info))))))
       (expected (upstream-safe (lambda () "letrec: can't bind x to (values 1 2)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35722 actual expected ok?))
