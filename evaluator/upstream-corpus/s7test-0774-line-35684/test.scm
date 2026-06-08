;; Imported from upstream s7test.scm line 35684.
;; Original form:
;; (test (+ (with-input-from-string "123" (lambda () (values 1 2 3)))) 6)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (+ (with-input-from-string "123" (lambda () (values 1 2 3)))))))
       (expected (upstream-safe (lambda () 6)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35684 actual expected ok?))
