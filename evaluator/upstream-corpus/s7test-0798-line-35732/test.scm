;; Imported from upstream s7test.scm line 35732.
;; Original form:
;; (test (catch #t (lambda () (let ((x 1)) (set! x (values 1 2)))) (lambda (type info) (apply format #f info))) "(set! x (values 1 2)): too many arguments to set!")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t (lambda () (let ((x 1)) (set! x (values 1 2)))) (lambda (type info) (apply format #f info))))))
       (expected (upstream-safe (lambda () "(set! x (values 1 2)): too many arguments to set!")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35732 actual expected ok?))
