;; Imported from upstream s7test.scm line 20483.
;; Original form:
;; (test (output-port? *stdout*) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (output-port? *stdout*))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 20483 actual expected ok?))
