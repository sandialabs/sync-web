;; Imported from upstream s7test.scm line 5173.
;; Original form:
;; (test (let ((a 1)) (let ((a (lambda () (procedure? a)))) (a))) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((a 1)) (let ((a (lambda () (procedure? a)))) (a))))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5173 actual expected ok?))
