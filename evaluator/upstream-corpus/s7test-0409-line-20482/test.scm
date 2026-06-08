;; Imported from upstream s7test.scm line 20482.
;; Original form:
;; (test (output-port? (current-output-port)) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (output-port? (current-output-port)))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 20482 actual expected ok?))
