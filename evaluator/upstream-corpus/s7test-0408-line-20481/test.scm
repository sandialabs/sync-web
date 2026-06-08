;; Imported from upstream s7test.scm line 20481.
;; Original form:
;; (test (output-port? *stdin*) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (output-port? *stdin*))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 20481 actual expected ok?))
