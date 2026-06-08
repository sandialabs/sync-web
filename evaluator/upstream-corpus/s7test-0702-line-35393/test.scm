;; Imported from upstream s7test.scm line 35393.
;; Original form:
;; (test (let () (define (call-func func arg1 arg2) (define (call) (func arg1 arg2)) (call)) (call-func + 1 2.5) (call-func - 5 2)) 3)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let () (define (call-func func arg1 arg2) (define (call) (func arg1 arg2)) (call)) (call-func + 1 2.5) (call-func - 5 2)))))
       (expected (upstream-safe (lambda () 3)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35393 actual expected ok?))
