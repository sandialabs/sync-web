;; Imported from upstream s7test.scm line 20642.
;; Original form:
;; (test (call-with-input-string "" (lambda (p) (set! (port-position p) 2) (port-position p))) 0)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (call-with-input-string "" (lambda (p) (set! (port-position p) 2) (port-position p))))))
       (expected (upstream-safe (lambda () 0)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 20642 actual expected ok?))
