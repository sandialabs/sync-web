;; Imported from upstream s7test.scm line 35685.
;; Original form:
;; (test (+ (call-with-input-string "123" (lambda (p) (values 1 2 3)))) 6)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (+ (call-with-input-string "123" (lambda (p) (values 1 2 3)))))))
       (expected (upstream-safe (lambda () 6)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35685 actual expected ok?))
