;; Imported from upstream s7test.scm line 15120.
;; Original form:
;; (test (let ((l1 (list 1))) (let ((l2 (list l1 l1))) (object->string l2))) "((1) (1))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((l1 (list 1))) (let ((l2 (list l1 l1))) (object->string l2))))))
       (expected (upstream-safe (lambda () "((1) (1))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15120 actual expected ok?))
