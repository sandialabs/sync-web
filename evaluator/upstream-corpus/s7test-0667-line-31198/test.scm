;; Imported from upstream s7test.scm line 31198.
;; Original form:
;; (test ((lambda (arg) (arg #f 123)) or) 123)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () ((lambda (arg) (arg #f 123)) or))))
       (expected (upstream-safe (lambda () 123)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 31198 actual expected ok?))
