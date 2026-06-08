;; Imported from upstream s7test.scm line 35740.
;; Original form:
;; (test (catch #t (lambda () (with-let (values (curlet) 2) 3)) (lambda (type info) (apply format #f info))) 3)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t (lambda () (with-let (values (curlet) 2) 3)) (lambda (type info) (apply format #f info))))))
       (expected (upstream-safe (lambda () 3)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35740 actual expected ok?))
