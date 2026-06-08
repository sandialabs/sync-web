;; Imported from upstream s7test.scm line 20643.
;; Original form:
;; (test (call-with-input-string "123" (lambda (p) (set! (port-position p) 12) (port-position p))) 3)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (call-with-input-string "123" (lambda (p) (set! (port-position p) 12) (port-position p))))))
       (expected (upstream-safe (lambda () 3)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 20643 actual expected ok?))
