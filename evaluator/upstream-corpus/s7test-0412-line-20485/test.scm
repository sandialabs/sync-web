;; Imported from upstream s7test.scm line 20485.
;; Original form:
;; (test (output-port? *stderr*) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (output-port? *stderr*))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 20485 actual expected ok?))
