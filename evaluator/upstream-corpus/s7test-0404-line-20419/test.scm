;; Imported from upstream s7test.scm line 20419.
;; Original form:
;; (test (input-port? (current-input-port)) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (input-port? (current-input-port)))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 20419 actual expected ok?))
