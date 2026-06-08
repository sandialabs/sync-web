;; Imported from upstream s7test.scm line 35394.
;; Original form:
;; (test (let () (define (call-func arg1 arg2) (let ((func (if (= arg1 1) + -))) (define (call) (func arg1 arg2)) (call))) (call-func 1 2.5) (call-func 5 2)) 3)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let () (define (call-func arg1 arg2) (let ((func (if (= arg1 1) + -))) (define (call) (func arg1 arg2)) (call))) (call-func 1 2.5) (call-func 5 2)))))
       (expected (upstream-safe (lambda () 3)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35394 actual expected ok?))
