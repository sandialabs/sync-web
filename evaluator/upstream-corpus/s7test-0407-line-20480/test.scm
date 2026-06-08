;; Imported from upstream s7test.scm line 20480.
;; Original form:
;; (test (output-port? (current-input-port)) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (output-port? (current-input-port)))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 20480 actual expected ok?))
